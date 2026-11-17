//! The app log file: what `log!` says, also kept in a file the GUI, TUI and MCP server share (qurry.log, 3 × 5 MB at most), so a
//! bug report has the connects, tunnels, reconnects and housekeeping that led to it. Every line goes through `redact` first, on
//! stderr too. Nothing is written to a file until `init` ran, so tests and harnesses never touch the user's log folder.

use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The log folder: macOS ~/Library/Logs/Qurry, Linux $XDG_STATE_HOME/qurry (~/.local/state/qurry), Windows %LOCALAPPDATA%\Qurry\logs
pub fn dir() -> PathBuf {
    let home = || dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home().join("Library/Logs/Qurry")
    } else if cfg!(windows) {
        dirs::data_local_dir().unwrap_or_else(home).join("Qurry").join("logs")
    } else {
        dirs::state_dir().or_else(dirs::data_local_dir).unwrap_or_else(home).join("qurry")
    }
}

/// The current log file (older ones: qurry.1.log, qurry.2.log next to it): the one `init` opened (DIR/logs with `--config-dir`),
/// else the one in `dir()`
pub fn path() -> PathBuf {
    let g = SINK.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().map_or_else(dir, |s| s.dir.clone()).join("qurry.log")
}

/// A file grows to this, then it is renamed to qurry.1.log (and that one to qurry.2.log)
const CAP: u64 = 5 * 1024 * 1024;

struct Sink {
    dir: PathBuf,
    app: String,
    cap: u64,
    file: File,
    /// Held while a line is written or the files rotate: the GUI, TUI and MCP server may write at the same time
    lock: File,
    /// A write failed (a full disk): said once on stderr
    failed: bool,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Starts writing the log file of `app` ("gui", "tui" or "mcp"): once, at startup. With `--config-dir DIR` (a scratch or test
/// run, another set of data sources), into DIR/logs instead of the shared folder
pub fn init(app: &str, config_dir: Option<&Path>) {
    init_at(&config_dir.map_or_else(dir, |d| d.join("logs")), app);
}

/// init, into `dir` (tests: a temporary folder)
pub fn init_at(dir: &Path, app: &str) {
    let sink = Sink::open(dir, app, CAP);
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = match sink {
        Ok(s) => Some(s),
        Err(e) => {
            // Before the TUI takes the screen: said on the terminal whatever QUIET is
            eprintln!(
                "[qurry] can't write the log file {} ({e}); logging to stderr only. Make that folder writable by you, or delete it (Qurry creates it again).",
                dir.join("qurry.log").display()
            );
            None
        }
    };
}

fn open_private(path: &Path) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let f = o.open(path)?;
    // One made before (or by someone else's umask): the log is the user's only
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(f)
}

/// The log folder, 0700 (made again when someone deleted it while an app runs)
fn private_dir(dir: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(dir)?;
    #[cfg(unix)]
    std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

impl Sink {
    fn open(dir: &Path, app: &str, cap: u64) -> std::io::Result<Sink> {
        private_dir(dir)?;
        Ok(Sink {
            file: open_private(&dir.join("qurry.log"))?,
            lock: open_private(&dir.join("qurry.log.lock"))?,
            dir: dir.to_path_buf(),
            app: app.to_string(),
            cap,
            failed: false,
        })
    }

    /// One line, in one write, under the lock shared with the other apps' processes
    fn write(&mut self, msg: &str) -> std::io::Result<()> {
        // The folder was deleted: made again, with a lock file the other apps open too
        let lock = self.dir.join("qurry.log.lock");
        if !lock.exists() {
            private_dir(&self.dir)?;
            self.lock = open_private(&lock)?;
        }
        let ts = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
        let msg = msg.strip_prefix("[qurry] ").unwrap_or(msg).replace('\r', "\\r").replace('\n', "\\n");
        let line = format!("{ts} [{} {}] {msg}\n", self.app, std::process::id());
        // A lock that fails (a file system without locks) still writes: a rare duplicate rotation, never a lost line
        let locked = self.lock.lock().is_ok();
        let r = self.rotate(line.len() as u64).and_then(|_| self.file.write_all(line.as_bytes()));
        if locked {
            let _ = self.lock.unlock();
        }
        r
    }

    /// Under the lock: follows another process's rotation (the open file is no longer qurry.log), and rotates when this line
    /// would take the file past the cap
    fn rotate(&mut self, add: u64) -> std::io::Result<()> {
        let log = self.dir.join("qurry.log");
        let now = std::fs::metadata(&log).map(|m| m.len()).ok();
        // Appends of every process go to the end of the same file: a different length means it was renamed (or deleted)
        if now != Some(self.file.metadata()?.len()) {
            self.file = open_private(&log)?;
        }
        if now.unwrap_or(0) > 0 && now.unwrap_or(0) + add > self.cap {
            let _ = std::fs::rename(self.dir.join("qurry.1.log"), self.dir.join("qurry.2.log"));
            std::fs::rename(&log, self.dir.join("qurry.1.log"))?;
            self.file = open_private(&log)?;
        }
        Ok(())
    }
}

/// What `log!` does: the redacted line on stderr (unless QUIET) and in the log file (when `init` ran)
pub fn log(msg: &str) {
    let msg = redact(msg);
    if !crate::QUIET.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("{msg}");
    }
    let mut g = SINK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = g.as_mut()
        && let Err(e) = s.write(&msg)
        && !std::mem::replace(&mut s.failed, true)
        // Not over the TUI's screen (QUIET)
        && !crate::QUIET.load(std::sync::atomic::Ordering::Relaxed)
    {
        eprintln!("[qurry] can't write the log file {} ({e}). Free up disk space, or make that folder writable by you.", s.dir.join("qurry.log").display());
    }
}

/// Panics go to the log too (what and where), then to the hook set before (the TUI's restores the terminal): release builds
/// abort, and the TUI's screen hides stderr
pub fn log_panics() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        log(&p.to_string());
        hook(p);
    }));
}

/// The GUI's front-end errors the log takes per minute; past them one line says the rest of the minute's are left out (a render
/// loop can't flood the file)
pub const FRONTEND_PER_MINUTE: u32 = 10;

/// How many front-end errors went to the log in the current minute
struct FrontendLimit {
    since: Option<std::time::Instant>,
    n: u32,
}

impl FrontendLimit {
    /// Counts an error at `now`: its number in the current minute (from 1)
    fn admit(&mut self, now: std::time::Instant) -> u32 {
        if self.since.is_none_or(|s| now.duration_since(s) >= std::time::Duration::from_secs(60)) {
            *self = FrontendLimit { since: Some(now), n: 0 };
        }
        self.n += 1;
        self.n
    }
}

static FRONTEND: Mutex<FrontendLimit> = Mutex::new(FrontendLimit { since: None, n: 0 });

/// Cut to `max` characters, `…` when cut
fn cut(s: &str, max: usize) -> Cow<'_, str> {
    match s.char_indices().nth(max) {
        Some((i, _)) => Cow::Owned(format!("{}…", &s[..i])),
        None => Cow::Borrowed(s),
    }
}

/// The log line of a front-end error (`source`: boundary, error, unhandledrejection): its message and stacks only, cut short. The
/// GUI never sends query text or cell values; `log` redacts the line like any other
fn frontend_line(source: &str, message: &str, stack: Option<&str>, component_stack: Option<&str>) -> String {
    let mut line = format!("front-end {} error: {}", cut(source, 32), cut(message.trim(), 1000));
    if let Some(s) = stack.map(str::trim).filter(|s| !s.is_empty()) {
        line += &format!(" | stack: {}", cut(s, 4000));
    }
    if let Some(s) = component_stack.map(str::trim).filter(|s| !s.is_empty()) {
        line += &format!(" | components: {}", cut(s, 2000));
    }
    line
}

/// A GUI front-end error into the log (the log_frontend_error command), at most FRONTEND_PER_MINUTE a minute
pub fn frontend_error(source: &str, message: &str, stack: Option<&str>, component_stack: Option<&str>) {
    frontend_error_with(&mut FRONTEND.lock().unwrap_or_else(|e| e.into_inner()), std::time::Instant::now(), source, message, stack, component_stack);
}

fn frontend_error_with(limit: &mut FrontendLimit, now: std::time::Instant, source: &str, message: &str, stack: Option<&str>, component_stack: Option<&str>) {
    match limit.admit(now) {
        n if n <= FRONTEND_PER_MINUTE => log(&frontend_line(source, message, stack, component_stack)),
        n if n == FRONTEND_PER_MINUTE + 1 => log(&format!("front-end errors: more than {FRONTEND_PER_MINUTE} this minute, the rest of this minute's are suppressed")),
        _ => {}
    }
}

/// An error for the log: a server error by its SQLSTATE only, since its message can quote the statement or the values it was
/// given (a duplicate key's DETAIL, MySQL's "near '…'"); other errors (network, tunnel, files) in full
pub fn error_kind(e: &crate::error::AppError) -> String {
    match e {
        crate::error::AppError::Db(sqlx::Error::Database(d)) => format!("server error {}", d.code().as_deref().unwrap_or("(no SQLSTATE)")),
        e => e.to_string(),
    }
}

/// Keys whose value is a secret (key=value, key: value, "key": "value", ?key=value), compared case-insensitively. A key may end
/// a longer name: PGPASSWORD, sslpassword, sshPassword, SessionToken, SecretAccessKey, AWS_SECRET_ACCESS_KEY
const SECRET_KEYS: &[&str] = &[
    "password", "passwd", "pwd", "pass", "passphrase", "secret", "token", "api_key", "apikey", "access_key", "accesskey", "auth", "authorization", "private_key", "privatekey",
];

/// An auth header's scheme: the credential is the word after it (`Authorization: Bearer <token>`)
const AUTH_SCHEMES: &[&str] = &["basic", "bearer", "digest", "negotiate"];

/// The one redaction every log line goes through (stderr and the file): passwords, keys, tokens and SQL literals (values) are
/// replaced, and so are the names, hosts, users and databases of the data sources this process has read (hide)
pub fn redact(s: &str) -> Cow<'_, str> {
    let out = hidden(&detail_values(&quotes(&mysql_near(&key_values(&userinfo(&pem(s)))))));
    if out == s { Cow::Borrowed(s) } else { Cow::Owned(out) }
}

/// What the log never shows (lowercased, longest first) and the word in its place. Process-wide; per test thread in tests, so a
/// test's data sources can't change another test's lines
#[cfg(not(test))]
static HIDDEN: Mutex<Vec<(String, &'static str)>> = Mutex::new(Vec::new());
#[cfg(test)]
thread_local! {
    static HIDDEN: std::cell::RefCell<Vec<(String, &'static str)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn with_hidden<R>(f: impl FnOnce(&mut Vec<(String, &'static str)>) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut HIDDEN.lock().unwrap_or_else(|e| e.into_inner()));
    #[cfg(test)]
    return HIDDEN.with(|h| f(&mut h.borrow_mut()));
}

/// Keeps `cfg`'s name, host, user and database, and its tunnel's host, user, SSM target and AWS profile, out of the log from now on:
/// error text quotes them (`Can't connect to host:port`, `password authentication failed for user "u"`). Called for every data
/// source read from connections.json and every form connected (Test Connection)
pub fn hide(cfg: &crate::store::ConnectionConfig) {
    hide_word(&cfg.host, "<host>");
    hide_word(&cfg.user, "<user>");
    hide_word(&cfg.database, "<database>");
    hide_word(&cfg.name, "<name>");
    if let Some(s) = &cfg.ssh {
        hide_word(&s.host, "<ssh host>");
        hide_word(&s.user, "<ssh user>");
    }
    if let Some(s) = &cfg.ssm {
        hide_word(&s.target, "<ssm target>");
        hide_word(&s.profile, "<aws profile>");
    }
}

/// hide, one word. Shorter than 3 characters it says little and would replace parts of ordinary text, so it is kept
pub fn hide_word(w: &str, label: &'static str) {
    let w = w.trim().to_ascii_lowercase();
    if w.chars().count() < 3 {
        return;
    }
    with_hidden(|h| {
        if !h.iter().any(|(x, _)| *x == w) {
            h.push((w, label));
            // A host before a name it contains (shop-db.acme.test before shop)
            h.sort_by_key(|(x, _)| std::cmp::Reverse(x.len()));
        }
    });
}

/// The hidden words (whole words, any case) → their label, and the database of a `<data source id>::<database>` target
fn hidden(s: &str) -> String {
    let s = target_dbs(s);
    with_hidden(|h| h.iter().fold(s, |s, (w, label)| replace_word(&s, w, label)))
}

/// `<id>::orders` (a pool's or console's target: the source's id ends in a UUID) → `<id>::<database>`
fn target_dbs(s: &str) -> String {
    let uuid_end = |head: &str| {
        let b = head.as_bytes();
        b.len() >= 36 && b[b.len() - 36..].iter().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { *c == b'-' } else { c.is_ascii_hexdigit() })
    };
    let (mut out, mut rest) = (String::new(), s);
    while let Some(i) = rest.find("::") {
        let (head, tail) = (&rest[..i], &rest[i + 2..]);
        out.push_str(head);
        out.push_str("::");
        rest = tail;
        if uuid_end(head) {
            let end = tail.find(|c: char| c.is_whitespace() || ":,;()[]\"'".contains(c)).unwrap_or(tail.len());
            if end > 0 {
                out.push_str("<database>");
            }
            rest = &tail[end..];
        }
    }
    out.push_str(rest);
    out
}

/// Each whole-word `w` (lowercase) in `s`, compared ASCII case-insensitively → `with`
fn replace_word(s: &str, w: &str, with: &str) -> String {
    // ASCII lowercasing keeps every byte offset
    let lower = s.to_ascii_lowercase();
    let (mut out, mut last, mut from) = (String::new(), 0, 0);
    while let Some(i) = lower[from..].find(w).map(|i| i + from) {
        let end = i + w.len();
        if !s[..i].chars().next_back().is_some_and(word) && !s[end..].chars().next().is_some_and(word) {
            out.push_str(&s[last..i]);
            out.push_str(with);
            last = end;
            from = end;
        } else {
            from = i + w.chars().next().map_or(1, char::len_utf8);
        }
    }
    out.push_str(&s[last..]);
    out
}

/// `-----BEGIN … KEY-----` up to its END line (or the end) → one marker
fn pem(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("-----BEGIN") {
        out.push_str(&rest[..i]);
        out.push_str("-----BEGIN (redacted)-----");
        let after = &rest[i + 10..];
        rest = match after.find("-----END") {
            Some(j) => {
                let tail = &after[j + 8..];
                // Past the END line's closing dashes
                match tail.find("-----") {
                    Some(k) => &tail[k + 5..],
                    None => "",
                }
            }
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// `scheme://user:secret@host` → `scheme://user:***@host`
fn userinfo(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        out.push_str(&rest[..i + 3]);
        rest = &rest[i + 3..];
        // The authority, to the next blank or quote: a password may hold '/' or '?' unencoded
        let end = rest.find(|c: char| c.is_whitespace() || "\"'<>".contains(c)).unwrap_or(rest.len());
        if let Some(at) = rest[..end].rfind('@')
            && let Some(colon) = rest[..at].find(':')
        {
            out.push_str(&rest[..colon + 1]);
            out.push_str("***");
            rest = &rest[at..];
        }
    }
    out.push_str(rest);
    out
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `password=x`, `Password: x`, `"token":"x"`, `?api_key=x&` → the value as ***. A key ends a word (`ssh_password`, `PGPASSWORD`, not
/// `passwords`)
fn key_values(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let b = s.as_bytes();
    let mut out = String::new();
    let mut from = 0;
    let mut i = 0;
    while i < s.len() {
        let key = SECRET_KEYS.iter().filter(|k| lower[i..].starts_with(**k)).max_by_key(|k| k.len());
        let Some(key) = key.filter(|k| !lower[i + k.len()..].starts_with(word)) else {
            i += s[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let mut j = i + key.len();
        // A JSON key's closing quote, then the separator
        if b.get(j) == Some(&b'"') {
            j += 1;
        }
        let skip_ws = |mut j: usize| {
            while b.get(j).is_some_and(|c| *c == b' ' || *c == b'\t') {
                j += 1;
            }
            j
        };
        j = skip_ws(j);
        // MySQL's `(using password: YES)` only says whether one was sent
        if !matches!(b.get(j), Some(b'=' | b':')) || lower[..i].ends_with("using ") {
            i += key.len();
            continue;
        }
        j = skip_ws(j + 1);
        let unquoted = |j: usize| s[j..].find(|c: char| c.is_whitespace() || "&,;)}]".contains(c)).map_or(s.len(), |k| j + k);
        let end = match b.get(j) {
            Some(q @ (b'"' | b'\'')) => {
                let mut k = j + 1;
                while k < b.len() && b[k] != *q {
                    k += if b[k] == b'\\' { 2 } else { 1 };
                }
                (k + 1).min(b.len())
            }
            _ => match unquoted(j) {
                // `Basic <credential>`: the scheme and the word after it
                e if AUTH_SCHEMES.contains(&&lower[j..e]) && skip_ws(e) > e && skip_ws(e) < s.len() => unquoted(skip_ws(e)),
                e => e,
            },
        };
        if end > j {
            out.push_str(&s[from..j]);
            out.push_str("***");
            from = end;
        }
        i = end.max(i + 1);
    }
    out.push_str(&s[from..]);
    out
}

/// MySQL quotes the statement from where it failed (`near '…' at line 1`), its own quotes and all: one literal
fn mysql_near(s: &str) -> String {
    let Some(i) = s.find("near '") else { return s.to_string() };
    let end = s.rfind("' at line").filter(|e| *e > i + 5).map_or(s.len(), |e| e + 1);
    format!("{}near '…'{}", &s[..i], &s[end..])
}

/// Every SQL string literal ('…', E'…', N'…', $$…$$, $tag$…$tag$) → '…'. An apostrophe of English (didn't, Bob's) is no quote.
/// One left open hides the rest of the line
fn quotes(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '\'' {
            let prev = i.checked_sub(1).map(|p| cs[p]);
            // One letter before the quote is a prefix (N'…', E'…')
            let prefix = prev.is_some_and(char::is_alphabetic) && i.checked_sub(2).is_none_or(|p| !word(cs[p]));
            let suffix = cs[i + 1..].iter().take_while(|c| word(**c)).collect::<String>().to_lowercase();
            let contraction = !prefix
                && prev.is_some_and(char::is_alphabetic)
                && ["t", "s", "re", "ve", "ll", "d", "m"].contains(&suffix.as_str())
                && cs.get(i + 1 + suffix.chars().count()) != Some(&'\'');
            if let Some(n) = mysql_account(&cs, i) {
                out.extend(&cs[i..i + n]);
                i += n;
                continue;
            }
            if !contraction {
                let mut k = i + 1;
                loop {
                    match cs.get(k) {
                        None => break,
                        // MySQL's literals and E'…': a backslash escapes. A PostgreSQL literal that ends in one hides more, never less
                        Some('\\') => k += 2,
                        Some('\'') if cs.get(k + 1) == Some(&'\'') => k += 2,
                        Some('\'') => break,
                        _ => k += 1,
                    }
                }
                out.push_str("'…'");
                i = k + 1;
                continue;
            }
        }
        if c == '$' {
            // $tag$: a tag is empty or a name that doesn't start with a digit ($1 is a parameter)
            let mut k = i + 1;
            while cs.get(k).is_some_and(|c| word(*c)) && !cs[i + 1].is_ascii_digit() {
                k += 1;
            }
            if cs.get(k) == Some(&'$') {
                let tag: String = cs[i..=k].iter().collect();
                let body: String = cs[k + 1..].iter().collect();
                let end = body.find(&tag).map(|e| k + 1 + body[..e].chars().count() + tag.chars().count()).unwrap_or(cs.len());
                out.push_str("'…'");
                i = end;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// MySQL's `Access denied for user 'app'@'10.0.0.5'`: the account and the client host the server saw are kept (their length
/// from the quote at `i`)
fn mysql_account(cs: &[char], i: usize) -> Option<usize> {
    const BEFORE: &str = "denied for user ";
    let n = BEFORE.len();
    if i < n || cs[i - n..i].iter().collect::<String>() != BEFORE {
        return None;
    }
    let close = |from: usize| cs.get(from..)?.iter().position(|c| *c == '\'').map(|p| from + p);
    let user = close(i + 1)?;
    if cs.get(user + 1..user + 3) != Some(&['@', '\''][..]) {
        return None;
    }
    Some(close(user + 3)? + 1 - i)
}

/// PostgreSQL's `Key (cols)=(values)` and `Failing row contains (values)` → the values as …
fn detail_values(s: &str) -> String {
    const ROW: &str = "Failing row contains (";
    let mut out = String::new();
    let mut rest = s;
    loop {
        // Where it is, and its values' opening parenthesis
        let at = [rest.find(")=(").map(|i| (i, i + 2)), rest.find(ROW).map(|i| (i, i + ROW.len() - 1))].into_iter().flatten().min();
        let Some((_, open)) = at else { break };
        out.push_str(&rest[..=open]);
        let mut depth = 1;
        let mut end = rest.len();
        for (k, c) in rest[open + 1..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = open + 1 + k;
                break;
            }
        }
        out.push('…');
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_url_userinfo() {
        assert_eq!(redact("postgres://app:s3cret@db.local:5432/x"), "postgres://app:***@db.local:5432/x");
        assert_eq!(redact("mysql://root:pa/ss@h/db failed"), "mysql://root:***@h/db failed");
        assert_eq!(redact("redis://:tok@h:6379"), "redis://:***@h:6379");
        // No password: kept
        assert_eq!(redact("postgres://app@db.local/x"), "postgres://app@db.local/x");
    }

    #[test]
    fn redacts_key_values() {
        assert_eq!(redact("host=h password=s3cret user=u"), "host=h password=*** user=u");
        assert_eq!(redact(r#"{"user":"u","Password": "s3\"cret","token":"t0k"}"#), r#"{"user":"u","Password": ***,"token":***}"#);
        assert_eq!(redact("?api_key=abc&x=1&ssh_passphrase=pp"), "?api_key=***&x=1&ssh_passphrase=***");
        assert_eq!(redact("AWS_SECRET_ACCESS_KEY: wJalr SESSION_TOKEN=xyz"), "AWS_SECRET_ACCESS_KEY: *** SESSION_TOKEN=***");
        assert_eq!(redact("pwd='a b' auth=Basic"), "pwd=*** auth=***");
        // A key at the end of a longer name: libpq, env, camelCase JSON (AWS credentials, config)
        assert_eq!(redact("postgres://u:pw@h/db?sslpassword=x&sslmode=require"), "postgres://u:***@h/db?sslpassword=***&sslmode=require");
        assert_eq!(redact("PGPASSWORD=s3cret psql -h h"), "PGPASSWORD=*** psql -h h");
        assert_eq!(
            redact(r#"{"AccessKeyId":"AKIA","SecretAccessKey":"s3cret","SessionToken":"s3cret2","sshPassword":"s3cret3"}"#),
            r#"{"AccessKeyId":"AKIA","SecretAccessKey":***,"SessionToken":***,"sshPassword":***}"#
        );
        assert_eq!(redact("password\t=\ts3cret"), "password\t=\t***");
        // An auth scheme and the credential after it
        assert_eq!(redact("Authorization: Bearer abc.def x"), "Authorization: *** x");
        assert_eq!(redact("auth=Basic s3cretb64, next"), "auth=***, next");
        // MySQL says only whether a password was sent
        assert_eq!(redact("(using password: YES)"), "(using password: YES)");
        // Words that only start like a key are kept
        assert_eq!(redact("saved passwords: 2, Authentication failed"), "saved passwords: 2, Authentication failed");
        assert_eq!(redact("private_key: abc"), "private_key: ***");
    }

    #[test]
    fn redacts_pem_blocks() {
        let k = "key -----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY----- done";
        assert_eq!(redact(k), "key -----BEGIN (redacted)----- done");
        assert_eq!(redact("-----BEGIN RSA PRIVATE KEY-----\nMIIE"), "-----BEGIN (redacted)-----");
    }

    #[test]
    fn redacts_literals() {
        assert_eq!(redact("insert into t values ('s3cret', 42)"), "insert into t values ('…', 42)");
        assert_eq!(redact("x = 'it''s' and y = E'a\\'b'"), "x = '…' and y = E'…'");
        assert_eq!(redact("do $$ select 1 $$; f($fn$ body $fn$) $1"), "do '…'; f('…') $1");
        // Left open: the rest of the line
        assert_eq!(redact("where a = 'abc"), "where a = '…'");
        // MySQL quotes the statement from where it failed
        assert_eq!(
            redact("You have an error in your SQL syntax near ''s3cret-literal', 42)' at line 1"),
            "You have an error in your SQL syntax near '…' at line 1"
        );
        // Apostrophes in words are no quotes, but a prefixed literal is a literal
        assert_eq!(redact("the tunnel didn't answer; Bob's DB; they're"), "the tunnel didn't answer; Bob's DB; they're");
        assert_eq!(redact("note = N'secret' or c = _utf8mb4'abc'"), "note = N'…' or c = _utf8mb4'…'");
        // MySQL's backslash escape in a plain literal
        assert_eq!(redact("insert into t values ('it\\'s s3cret', 1)"), "insert into t values ('…', 1)");
    }

    #[test]
    fn keeps_mysql_account_of_access_denied() {
        assert_eq!(
            redact("error returned from database: 1045 (28000): Access denied for user 'app'@'10.0.0.5' (using password: YES)"),
            "error returned from database: 1045 (28000): Access denied for user 'app'@'10.0.0.5' (using password: YES)"
        );
        assert_eq!(redact("Access denied for user 'app'@'%' to database 'crm'"), "Access denied for user 'app'@'%' to database '…'");
    }

    #[test]
    fn redacts_pg_detail() {
        assert_eq!(redact("DETAIL: Key (email, id)=(a@b.c, 7) already exists."), "DETAIL: Key (email, id)=(…) already exists.");
        assert_eq!(redact("Failing row contains (1, secret, null)."), "Failing row contains (…).");
    }

    #[test]
    fn keeps_plain_lines_borrowed() {
        let s = "connect 6f1c… ok in 12 ms (db.local:5432 as app)";
        assert!(matches!(redact(s), Cow::Borrowed(_)));
    }

    /// A data source made up for the tests below: every field that says where it points is a word no other test uses
    fn acme(name: &str, host: &str, ssh: Option<serde_json::Value>) -> crate::store::ConnectionConfig {
        serde_json::from_value(serde_json::json!({ "id": "", "name": name, "kind": "postgres", "host": host, "port": 59431, "user": "ledger_admin",
            "database": "orders_ledger", "ssh": ssh })).unwrap()
    }

    /// hide: a data source's name, host, user and database (and its tunnel's) never show, whole words in any case; other text and
    /// longer words that only contain one are kept
    #[test]
    fn hides_data_source_words() {
        hide(&acme("Acme Ledger Prod", "shop-db.acme.test", Some(serde_json::json!({ "enabled": true, "host": "jump.acme.test", "user": "ops-jumper" }))));
        assert_eq!(
            redact("Can't connect to Shop-DB.acme.test:59431 — the server didn't answer within 10s"),
            "Can't connect to <host>:59431 — the server didn't answer within 10s"
        );
        assert_eq!(redact(r#"password authentication failed for user "ledger_admin""#), r#"password authentication failed for user "<user>""#);
        assert_eq!(redact("Access denied for user 'ledger_admin'@'10.0.0.5' (using password: YES)"), "Access denied for user '<user>'@'10.0.0.5' (using password: YES)");
        assert_eq!(redact(r#"database "orders_ledger" does not exist; ledger_admins orders_ledger2"#), r#"database "<database>" does not exist; ledger_admins orders_ledger2"#);
        assert_eq!(
            redact("SSH login OK, but ops-jumper@jump.acme.test cannot reach shop-db.acme.test:59431"),
            "SSH login OK, but <ssh user>@<ssh host> cannot reach <host>:59431"
        );
        assert_eq!(redact("Acme Ledger Prod was disconnected"), "<name> was disconnected");
        // Too short to hide, and words that are no data source's
        hide_word("db", "<database>");
        assert_eq!(redact("the db pool of 6f1c closed"), "the db pool of 6f1c closed");
        // The database of a target id, not Rust paths
        assert_eq!(
            redact("pool open 6f1c2b0e-1a2b-4c3d-8e9f-0a1b2c3d4e5f::archive_2026 failed: x; tmp-6f1c2b0e-1a2b-4c3d-8e9f-0a1b2c3d4e5f::crm, core::option"),
            "pool open 6f1c2b0e-1a2b-4c3d-8e9f-0a1b2c3d4e5f::<database> failed: x; tmp-6f1c2b0e-1a2b-4c3d-8e9f-0a1b2c3d4e5f::<database>, core::option"
        );
    }

    /// Save, connect, pool open, a failing tunnel and Test Connection of an unsaved form, as the apps run them: the file names each
    /// data source by its id, and none of their names, hosts, ports, users or databases (error text included)
    #[tokio::test]
    async fn log_has_no_data_source_details() {
        let _one = SET.lock().await;
        let t = tempfile::tempdir().unwrap();
        init_at(&t.path().join("logs"), "gui");
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let direct = st.save_connection(acme("Acme Ledger Prod", "shop-db.acme.test", None), Some("Pw-unused-1".into()), None).await.unwrap();
        let ssh = serde_json::json!({ "enabled": true, "host": "jump.acme.test", "port": 22, "user": "ops-jumper", "auth": "password", "keyPath": "" });
        let tunneled = st.save_connection(acme("Acme Ledger Jump", "inner-db.acme.test", Some(ssh)), Some("Pw-unused-1".into()), Some("Pw-unused-2".into())).await.unwrap();
        assert!(st.connect(direct.id.clone()).await.is_err());
        assert!(st.connect(tunneled.id.clone()).await.is_err());
        assert!(st.test_connection(acme("Acme Draft", "draft-db.acme.test", None), Some("Pw-unused-1".into()), None).await.is_err());
        *SINK.lock().unwrap() = None;
        let text = std::fs::read_to_string(t.path().join("logs/qurry.log")).unwrap();
        println!("{text}");
        for w in ["Acme", "acme.test", "ledger_admin", "orders_ledger", "ops-jumper", "Pw-unused"] {
            assert!(!text.to_lowercase().contains(&w.to_lowercase()), "{w} in\n{text}");
        }
        // The port only next to <host> in a driver's error text, never in Qurry's own words
        assert!(text.lines().filter(|l| l.contains("59431")).all(|l| l.contains("<host>:59431")), "{text}");
        for (what, id) in [("save_connection", "(new)"), ("connect", direct.id.as_str()), ("pool open", direct.id.as_str()), ("connect", tunneled.id.as_str()), ("SSH tunnel of", tunneled.id.as_str()), ("connect", "(unsaved) failed")] {
            assert!(text.lines().any(|l| l.contains(&format!("] {what} {id}"))), "{what} {id} in\n{text}");
        }
        assert!(text.lines().any(|l| l.contains(&format!("] SSH tunnel of {} failed after ", tunneled.id))), "{text}");
    }

    fn lines(dir: &Path, name: &str) -> Vec<String> {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default().lines().map(String::from).collect()
    }

    #[test]
    fn line_format_and_mode() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("logs");
        let mut s = Sink::open(&dir, "tui", CAP).unwrap();
        s.write("[qurry] two\nlines").unwrap();
        let l = lines(&dir, "qurry.log");
        assert_eq!(l.len(), 1);
        let (ts, rest) = l[0].split_once(' ').unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(ts).is_ok(), "{ts}");
        assert!(ts.contains('.'), "milliseconds: {ts}");
        assert_eq!(rest, format!("[tui {}] two\\nlines", std::process::id()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join("qurry.log")).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn makes_a_deleted_folder_again() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("logs");
        let mut s = Sink::open(&dir, "tui", CAP).unwrap();
        s.write("before").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        s.write("after").unwrap();
        assert_eq!(lines(&dir, "qurry.log").len(), 1);
        assert!(dir.join("qurry.log.lock").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn rotation_keeps_three_files() {
        let t = tempfile::tempdir().unwrap();
        let mut s = Sink::open(t.path(), "gui", 200).unwrap();
        // Another process writing too: it follows the renames
        let mut other = Sink::open(t.path(), "mcp", 200).unwrap();
        for i in 0..100 {
            s.write(&format!("line {i}")).unwrap();
            other.write(&format!("other {i}")).unwrap();
        }
        let mut names: Vec<String> = std::fs::read_dir(t.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|n| n.ends_with(".log")).collect();
        names.sort();
        assert_eq!(names, ["qurry.1.log", "qurry.2.log", "qurry.log"]);
        for n in &names {
            assert!(std::fs::metadata(t.path().join(n)).unwrap().len() <= 200, "{n}");
        }
        assert!(lines(t.path(), "qurry.log").last().unwrap().ends_with("other 99"));
    }

    #[test]
    fn concurrent_writers_write_whole_lines() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().to_path_buf();
        let threads: Vec<_> = (0..6)
            .map(|n| {
                let dir = dir.clone();
                // Each its own Sink, as separate processes have
                std::thread::spawn(move || {
                    let mut s = Sink::open(&dir, "tui", 1 << 30).unwrap();
                    for i in 0..200 {
                        s.write(&format!("writer {n} line {i} {}", "x".repeat(300))).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let l = lines(&dir, "qurry.log");
        assert_eq!(l.len(), 1200);
        assert!(l.iter().all(|l| l.ends_with(&"x".repeat(300)) && l.matches("writer").count() == 1));
    }

    /// The tests that set the process's log file, one at a time (other tests' lines may land in it meanwhile)
    static SET: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A GUI front-end error (log_frontend_error): message and stacks redacted and cut, at most FRONTEND_PER_MINUTE a minute, then
    /// one line, then again once the minute is over
    #[test]
    fn frontend_errors_are_masked_and_limited() {
        let _one = SET.blocking_lock();
        let t = tempfile::tempdir().unwrap();
        init_at(t.path(), "gui");
        let mut limit = FrontendLimit { since: None, n: 0 };
        let t0 = std::time::Instant::now();
        let long = "x".repeat(10_000);
        frontend_error_with(
            &mut limit,
            t0,
            "boundary",
            "Failed near postgres://app:Pw-s3cret-1@db.local/x where name = 's3cret-literal'",
            Some(&format!("Error: boom\n    at render (index.js:1:2)\n{long}")),
            Some("\n    at ConsolePane\n    at ErrorBoundary"),
        );
        for _ in 0..FRONTEND_PER_MINUTE + 5 {
            frontend_error_with(&mut limit, t0 + std::time::Duration::from_secs(30), "error", "loop", None, None);
        }
        frontend_error_with(&mut limit, t0 + std::time::Duration::from_secs(61), "unhandledrejection", "after the minute", None, Some(""));
        *SINK.lock().unwrap() = None;
        // Other tests' lines may land in the file meanwhile
        let l: Vec<String> = lines(t.path(), "qurry.log").into_iter().filter(|x| x.contains("] front-end ")).collect();
        let text = l.join("\n");
        assert!(!text.contains("s3cret") && !text.contains("Pw-"), "{text}");
        assert!(l[0].contains("] front-end boundary error: Failed near postgres://app:***@db.local/x where name = '…' | stack: Error: boom\\n    at render (index.js:1:2)"), "{}", l[0]);
        assert!(l[0].ends_with("x… | components: at ConsolePane\\n    at ErrorBoundary"), "{}", l[0]);
        assert!(l[0].len() < 5_000, "cut: {}", l[0].len());
        // The first and FRONTEND_PER_MINUTE - 1 of the loop, the one line, nothing more until the minute is over
        assert_eq!(l.iter().filter(|x| x.ends_with("] front-end error error: loop")).count() as u32, FRONTEND_PER_MINUTE - 1, "{text}");
        assert_eq!(l.iter().filter(|x| x.ends_with(&format!("] front-end errors: more than {FRONTEND_PER_MINUTE} this minute, the rest of this minute's are suppressed"))).count(), 1, "{text}");
        assert!(l.last().unwrap().ends_with("] front-end unhandledrejection error: after the minute"), "{text}");
        assert_eq!(l.len() as u32, FRONTEND_PER_MINUTE + 2, "{text}");
    }

    /// log_panics: a panic's message and place reach the file (redacted), and the hook set before still runs
    #[test]
    fn panics_are_logged() {
        let _one = SET.blocking_lock();
        let t = tempfile::tempdir().unwrap();
        init_at(t.path(), "tui");
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let r = ran.clone();
        std::panic::set_hook(Box::new(move |_| r.store(true, std::sync::atomic::Ordering::SeqCst)));
        log_panics();
        let _ = std::panic::catch_unwind(|| panic!("boom password=s3cret"));
        // Back to the default hook for the other tests
        let _ = std::panic::take_hook();
        *SINK.lock().unwrap() = None;
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
        let text = std::fs::read_to_string(t.path().join("qurry.log")).unwrap();
        assert!(text.contains("] panicked at crates/core/src/logfile.rs:") && text.contains(":\\nboom password=***\n"), "{text}");
    }

    /// Through log! itself, as the apps write it
    #[test]
    fn log_macro_never_writes_secrets() {
        let _one = SET.blocking_lock();
        let t = tempfile::tempdir().unwrap();
        init_at(t.path(), "gui");
        // Show Log reveals the file init opened
        assert_eq!(path(), t.path().join("qurry.log"));
        crate::log!("[qurry] save_connection dsn=postgres://app:Pw-s3cret-1@db.local:5432/x password=Pw-s3cret-1");
        crate::log!("[qurry] save ssh {{\"host\":\"bastion\",\"passphrase\":\"s3cret-phrase\",\"key\":\"-----BEGIN OPENSSH PRIVATE KEY-----\\nAAAAs3cret\\n-----END OPENSSH PRIVATE KEY-----\"}}");
        crate::log!("[qurry] run failed: insert into t (a, b) values ('s3cret-literal', 42)");
        *SINK.lock().unwrap() = None;
        let text = std::fs::read_to_string(t.path().join("qurry.log")).unwrap();
        assert!(!text.contains("s3cret") && !text.contains("Pw-"), "{text}");
        assert!(text.contains("] save_connection dsn=postgres://app:***@db.local:5432/x password=***\n"), "{text}");
        assert!(text.contains("] save ssh {\"host\":\"bastion\",\"passphrase\":***,\"key\":\"-----BEGIN (redacted)-----\"}\n"), "{text}");
        assert!(text.contains("] run failed: insert into t (a, b) values ('…', 42)\n"), "{text}");
    }

    /// A statement's notice collector (crate::notice::collect) is a tracing dispatcher: Qurry's own lines don't go through tracing,
    /// so they still reach the file while it runs
    #[tokio::test]
    async fn log_lines_pass_a_notice_collector() {
        let _one = SET.lock().await;
        let t = tempfile::tempdir().unwrap();
        init_at(t.path(), "gui");
        let ((), notices) = crate::notice::collect(async { crate::log!("[qurry] while collecting notices") }).await;
        *SINK.lock().unwrap() = None;
        assert!(notices.is_empty());
        let text = std::fs::read_to_string(t.path().join("qurry.log")).unwrap();
        assert!(text.contains("] while collecting notices\n"), "{text}");
    }

    /// A save and connect to a real server: the file has the connect with its time, never the password.
    /// QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres[:<password>] cargo test -p qurry-core logfile_real_connect -- --ignored
    /// Without a password (CI's trust auth) a made-up one is saved: the server ignores it, and the file still must not show it.
    #[tokio::test]
    #[ignore]
    async fn logfile_real_connect() {
        let Some(pg) = crate::testenv::pg() else { return };
        let password = pg.password.clone().unwrap_or_else(|| "Pw-trust-unused-1".into());
        let _one = SET.lock().await;
        let t = tempfile::tempdir().unwrap();
        init_at(&t.path().join("logs"), "tui");
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let mut cfg = pg.cfg;
        cfg.id = String::new();
        let saved = st.save_connection(cfg, Some(password.clone()), None).await.unwrap();
        st.connect(saved.id.clone()).await.unwrap();
        st.disconnect(saved.id.clone()).await.unwrap();
        *SINK.lock().unwrap() = None;
        let text = std::fs::read_to_string(t.path().join("logs/qurry.log")).unwrap();
        println!("{text}");
        assert!(!text.contains(&password), "{text}");
        assert!(text.lines().any(|l| l.contains(&format!("] connect {} ok in ", saved.id)) && l.ends_with(" ms")), "{text}");
        assert!(text.lines().any(|l| l.contains(&format!("] pool open {} ok in ", saved.id))), "{text}");
        assert!(text.lines().any(|l| l.contains(&format!("] closed {}: 1 pool(s)", saved.id))), "{text}");
    }
}
