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
/// replaced, names, hosts and users are kept
pub fn redact(s: &str) -> Cow<'_, str> {
    let out = detail_values(&quotes(&mysql_near(&key_values(&userinfo(&pem(s))))));
    if out == s { Cow::Borrowed(s) } else { Cow::Owned(out) }
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
