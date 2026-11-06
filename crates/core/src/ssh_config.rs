//! ~/.ssh/config, read the way OpenSSH reads it for what the SSH tunnel (tunnel.rs) uses: Host blocks (patterns with * ? and !,
//! first value wins), HostName, User, Port, IdentityFile, IdentityAgent, ProxyJump, Include. Nothing here runs a program:
//! ProxyCommand is refused, except the common `ssh -W %h:%p <jump host>`, which is the same route as `ProxyJump <jump host>`.
//! Match blocks are skipped (their settings are not applied).
use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// One setting line: the Host patterns of its block (None = before any Host line: every host), the keyword (lowercase), the value, and where it is
#[derive(Debug, Clone)]
struct Entry {
    hosts: Option<Vec<String>>,
    key: String,
    value: String,
    at: String,
}

#[derive(Debug, Default)]
pub struct SshConfigFile {
    entries: Vec<Entry>,
}

/// A host's settings (the first value of each keyword that applies; IdentityFile: every one)
#[derive(Debug, Default, PartialEq)]
struct HostConfig {
    host_name: Option<(String, String)>,
    user: Option<(String, String)>,
    port: Option<(String, String)>,
    identity_files: Vec<(String, String)>,
    identity_agent: Option<(String, String)>,
    proxy_jump: Option<(String, String)>,
    proxy_command: Option<(String, String)>,
}

/// Where the SSH agent is
#[derive(Debug, Clone, PartialEq)]
pub enum Agent {
    /// SSH_AUTH_SOCK
    Env,
    Socket(PathBuf),
    /// IdentityAgent none
    Off,
}

/// One SSH server on the way: the ProxyJump hosts first, the data source's SSH host last
#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    /// The real host name (HostName), what known_hosts is checked against
    pub host: String,
    pub port: u16,
    pub user: String,
    /// IdentityFile keys (or OpenSSH's default ones that exist when none is set), tried after the agent
    pub identity_files: Vec<PathBuf>,
    pub agent: Agent,
}

/// More hops than this is a ProxyJump loop (a jump host that names itself, directly or through others)
const MAX_HOPS: usize = 10;
const MAX_INCLUDE_DEPTH: usize = 16;

impl SshConfigFile {
    /// QURRY_SSH_CONFIG (tests), otherwise ~/.ssh/config. A missing file is an empty config
    pub fn load() -> AppResult<Self> {
        let Some(home) = dirs::home_dir() else { return Ok(Self::default()) };
        let path = std::env::var_os("QURRY_SSH_CONFIG").map(PathBuf::from).unwrap_or_else(|| home.join(".ssh").join("config"));
        Self::read(&path, &home)
    }

    pub fn read(path: &Path, home: &Path) -> AppResult<Self> {
        let mut entries = Vec::new();
        parse_file(path, home, &None, 0, &mut entries)?;
        Ok(Self { entries })
    }

    #[cfg(test)]
    fn parse(text: &str, home: &Path) -> AppResult<Self> {
        let mut entries = Vec::new();
        parse_text(text, "config", home, &None, 0, &mut entries)?;
        Ok(Self { entries })
    }

    fn lookup(&self, host: &str) -> HostConfig {
        let mut c = HostConfig::default();
        for e in self.entries.iter().filter(|e| e.hosts.as_ref().is_none_or(|p| host_matches(p, host))) {
            let v = (e.value.clone(), e.at.clone());
            let first = |slot: &mut Option<(String, String)>| {
                if slot.is_none() {
                    *slot = Some(v.clone());
                }
            };
            match e.key.as_str() {
                "hostname" => first(&mut c.host_name),
                "user" => first(&mut c.user),
                "port" => first(&mut c.port),
                "identityagent" => first(&mut c.identity_agent),
                "proxyjump" => first(&mut c.proxy_jump),
                "proxycommand" => first(&mut c.proxy_command),
                "identityfile" => c.identity_files.push(v),
                _ => {}
            }
        }
        c
    }

    /// The SSH servers to go through to reach `host`: its ProxyJump hosts (and theirs) first, `host` last. `port` / `user` given by the
    /// data source win over the config's (None: the config's, then 22 / `local_user`)
    pub fn route(&self, host: &str, port: Option<u16>, user: Option<&str>, home: &Path, local_user: &str) -> AppResult<Vec<Hop>> {
        let mut hops = Vec::new();
        self.route_into(host, port, user, home, local_user, 0, &mut hops)?;
        Ok(hops)
    }

    #[allow(clippy::too_many_arguments)]
    fn route_into(&self, alias: &str, port: Option<u16>, user: Option<&str>, home: &Path, local_user: &str, depth: usize, hops: &mut Vec<Hop>) -> AppResult<()> {
        if depth >= MAX_HOPS || hops.len() >= MAX_HOPS {
            return Err(AppError::Msg(format!("~/.ssh/config: ProxyJump goes through more than {MAX_HOPS} hosts on the way to {alias} — check it for a loop")));
        }
        let alias = check_host(alias, "SSH host")?;
        let c = self.lookup(&alias);
        let host = match &c.host_name {
            Some((v, at)) => check_host(&expand_tokens(v, at, &[('h', &alias)])?, at)?,
            None => alias.clone(),
        };
        let port = match (port, &c.port) {
            (Some(p), _) => p,
            (None, Some((v, at))) => v.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| AppError::Msg(format!("{at}: Port must be a number from 1 to 65535, not \"{v}\"")))?,
            (None, None) => 22,
        };
        let user = match (user.filter(|u| !u.trim().is_empty()), &c.user) {
            (Some(u), _) => check_user(u.trim(), "SSH user")?,
            (None, Some((v, at))) => check_user(v, at)?,
            (None, None) => check_user(local_user, "the local user name")?,
        };
        let home_s = home.to_string_lossy().to_string();
        let port_s = port.to_string();
        let tokens = [('d', home_s.as_str()), ('u', local_user), ('h', host.as_str()), ('r', user.as_str()), ('p', port_s.as_str())];
        let mut identity_files = Vec::new();
        for (v, at) in &c.identity_files {
            identity_files.push(file_path(&expand_tokens(v, at, &tokens)?, home));
        }
        if identity_files.is_empty() {
            identity_files = ["id_ed25519", "id_ecdsa", "id_rsa"].iter().map(|f| home.join(".ssh").join(f)).filter(|p| p.exists()).collect();
        }
        let agent = match &c.identity_agent {
            None => Agent::Env,
            Some((v, _)) if v.eq_ignore_ascii_case("none") => Agent::Off,
            Some((v, _)) if v == "SSH_AUTH_SOCK" || v == "$SSH_AUTH_SOCK" => Agent::Env,
            Some((v, at)) => Agent::Socket(file_path(&expand_tokens(v, at, &tokens)?, home)),
        };
        let jumps = match (&c.proxy_jump, &c.proxy_command) {
            (Some((v, _)), _) if v.eq_ignore_ascii_case("none") => None,
            (Some((v, at)), _) => Some((v.clone(), at.clone())),
            (None, Some((v, _))) if v.eq_ignore_ascii_case("none") => None,
            (None, Some((v, at))) => Some((proxy_command_jump(v, at)?, at.clone())),
            (None, None) => None,
        };
        if let Some((list, at)) = jumps {
            for j in list.split(',') {
                let (ju, jh, jp) = parse_destination(j.trim(), &at)?;
                self.route_into(&jh, jp, ju.as_deref(), home, local_user, depth + 1, hops)?;
            }
        }
        hops.push(Hop { host, port, user, identity_files, agent });
        Ok(())
    }
}

/// `~/x` → home/x; a relative path is relative to the home folder
fn file_path(v: &str, home: &Path) -> PathBuf {
    match v.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if v == "~" => home.to_path_buf(),
        None => home.join(v),
    }
}

/// %h %p %r %u %d %% as OpenSSH expands them; any other token is refused (Qurry doesn't know what it would stand for)
fn expand_tokens(v: &str, at: &str, tokens: &[(char, &str)]) -> AppResult<String> {
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(t) => match tokens.iter().find(|(k, _)| *k == t) {
                Some((_, s)) => out.push_str(s),
                None => return Err(AppError::Msg(format!("{at}: the token %{t} in \"{v}\" isn't supported here (Qurry knows %h %p %r %u %d %%)"))),
            },
            None => return Err(AppError::Msg(format!("{at}: \"{v}\" ends with a lone %"))),
        }
    }
    Ok(out)
}

/// A host name or address: no spaces, control characters or a leading - (it never reaches a shell or a command line, but such a value is a mistake or an attack)
fn check_host(v: &str, at: &str) -> AppResult<String> {
    let v = v.trim();
    let bare = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(v);
    let ok = !bare.is_empty() && bare.len() <= 255 && !bare.starts_with('-') && bare.chars().all(|c| c.is_alphanumeric() || ".-_:%".contains(c));
    if ok { Ok(bare.to_string()) } else { Err(AppError::Msg(format!("{at}: \"{}\" isn't a valid host name", v.escape_debug()))) }
}

fn check_user(v: &str, at: &str) -> AppResult<String> {
    let ok = !v.is_empty() && v.len() <= 255 && !v.starts_with('-') && !v.chars().any(|c| c.is_whitespace() || c.is_control());
    if ok { Ok(v.to_string()) } else { Err(AppError::Msg(format!("{at}: \"{}\" isn't a valid user name", v.escape_debug()))) }
}

/// One ProxyJump entry or a --ssh value: `[user@]host[:port]` or `ssh://[user@]host[:port]` (an IPv6 address in brackets)
pub fn parse_destination(v: &str, at: &str) -> AppResult<(Option<String>, String, Option<u16>)> {
    let v = v.strip_prefix("ssh://").unwrap_or(v);
    let (user, rest) = match v.rsplit_once('@') {
        Some((u, r)) => (Some(check_user(u, at)?), r),
        None => (None, v),
    };
    let bad_port = |p: &str| AppError::Msg(format!("{at}: ProxyJump port \"{p}\" must be a number from 1 to 65535"));
    let port = |p: &str| p.parse::<u16>().ok().filter(|n| *n > 0).ok_or_else(|| bad_port(p));
    let (host, port) = if let Some(r) = rest.strip_prefix('[') {
        let (h, tail) = r.split_once(']').ok_or_else(|| AppError::Msg(format!("{at}: \"{v}\" has an unclosed [")))?;
        match tail.strip_prefix(':') {
            Some(p) => (h.to_string(), Some(port(p)?)),
            None if tail.is_empty() => (h.to_string(), None),
            None => return Err(AppError::Msg(format!("{at}: \"{v}\" isn't [address]:port"))),
        }
    } else {
        match rest.split_once(':') {
            Some((h, p)) => (h.to_string(), Some(port(p)?)),
            None => (rest.to_string(), None),
        }
    };
    Ok((user, check_host(&host, at)?, port))
}

/// ProxyCommand would need a shell. Only `ssh [-q] -W %h:%p [user@]jump` is taken (as ProxyJump jump); everything else is refused
fn proxy_command_jump(v: &str, at: &str) -> AppResult<String> {
    let words: Vec<&str> = v.split_whitespace().collect();
    let refuse = || {
        AppError::Msg(format!(
            "{at}: ProxyCommand isn't supported (Qurry doesn't run commands from ~/.ssh/config). Use ProxyJump instead, or put \"ProxyCommand none\" in a Host block above this one to connect directly. Only \"ssh -W %h:%p <jump host>\" is understood."
        ))
    };
    let (Some(first), rest) = (words.first(), words.get(1..).unwrap_or_default()) else { return Err(refuse()) };
    if !(*first == "ssh" || first.ends_with("/ssh")) {
        return Err(refuse());
    }
    let mut jump = None;
    let mut forward = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i] {
            "-q" => {}
            "-W" if rest.get(i + 1).is_some_and(|w| *w == "%h:%p" || *w == "[%h]:%p") => {
                forward = true;
                i += 1;
            }
            w if !w.starts_with('-') && jump.is_none() => jump = Some(w),
            _ => return Err(refuse()),
        }
        i += 1;
    }
    match (forward, jump) {
        (true, Some(j)) => Ok(j.to_string()),
        _ => Err(refuse()),
    }
}

/// Patterns of a Host line: matches if a pattern matches and no !pattern does (case-insensitive, * and ?)
fn host_matches(patterns: &[String], host: &str) -> bool {
    let host = host.to_lowercase();
    let mut hit = false;
    for p in patterns {
        let p = p.to_lowercase();
        match p.strip_prefix('!') {
            Some(neg) if glob(neg, &host) => return false,
            Some(_) => {}
            None => hit |= glob(&p, &host),
        }
    }
    hit
}

fn glob(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    // Iterative * backtracking
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < s.len() {
        if i < p.len() && (p[i] == '?' || p[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(st) = star {
            i = st + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

/// A line's keyword and its value (`Key value`, `Key=value`, `Key = "value"`)
fn split_line(line: &str) -> Option<(String, String)> {
    let i = line.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(line.len());
    let key = line[..i].to_lowercase();
    let rest = line[i..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    (!key.is_empty()).then(|| (key, rest.to_string()))
}

fn unquote(v: &str) -> String {
    v.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(v).to_string()
}

fn parse_file(path: &Path, home: &Path, hosts: &Option<Vec<String>>, depth: usize, out: &mut Vec<Entry>) -> AppResult<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(AppError::Msg(format!("Cannot read {}: {e}", path.display()))),
    };
    parse_text(&text, &path.display().to_string(), home, hosts, depth, out)
}

fn parse_text(text: &str, name: &str, home: &Path, hosts: &Option<Vec<String>>, depth: usize, out: &mut Vec<Entry>) -> AppResult<()> {
    let mut hosts = hosts.clone();
    let mut in_match = false;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = split_line(line) else { continue };
        let at = format!("{name}:{}", n + 1);
        match key.as_str() {
            "host" => {
                hosts = Some(value.split_whitespace().map(unquote).collect());
                in_match = false;
            }
            "match" => in_match = true,
            _ if in_match => {}
            "include" => {
                if depth >= MAX_INCLUDE_DEPTH {
                    return Err(AppError::Msg(format!("{at}: Include goes deeper than {MAX_INCLUDE_DEPTH} files — check it for a loop")));
                }
                for p in value.split_whitespace().map(unquote) {
                    for f in include_files(&p, home)? {
                        parse_file(&f, home, &hosts, depth + 1, out)?;
                    }
                }
            }
            _ => out.push(Entry { hosts: hosts.clone(), key, value: unquote(&value), at }),
        }
    }
    Ok(())
}

/// An Include argument: ~ or relative to ~/.ssh; * and ? in the file name pick the matching files in name order
fn include_files(p: &str, home: &Path) -> AppResult<Vec<PathBuf>> {
    let path = match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if p.starts_with('/') => PathBuf::from(p),
        None => home.join(".ssh").join(p),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !name.contains(['*', '?']) {
        return Ok(vec![path]);
    }
    let Some(dir) = path.parent() else { return Ok(vec![]) };
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| glob(&name, &e.file_name().to_string_lossy())).map(|e| e.path()).filter(|p| p.is_file()).collect(),
        Err(_) => vec![],
    };
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/me")
    }

    fn route(conf: &str, host: &str) -> AppResult<Vec<Hop>> {
        SshConfigFile::parse(conf, &home())?.route(host, None, None, &home(), "me")
    }

    fn hosts(r: &[Hop]) -> Vec<String> {
        r.iter().map(|h| format!("{}@{}:{}", h.user, h.host, h.port)).collect()
    }

    #[test]
    fn alias_resolves_host_name_user_port_and_first_value_wins() {
        let conf = "# comment\nHost bastion prod-*\n  HostName bastion.example.com\n  User = ops\n  Port=2222\n\nHost *\n  User nobody\n  Port 22\n  HostName wrong.example\n";
        assert_eq!(hosts(&route(conf, "bastion").unwrap()), ["ops@bastion.example.com:2222"]);
        // A pattern: the second Host block only fills what the first left unset
        let r = route("Host prod-*\n User ops\nHost *\n Port 2200\n", "prod-db1").unwrap();
        assert_eq!(hosts(&r), ["ops@prod-db1:2200"]);
        // Nothing matches: the name as given, port 22, the local user
        assert_eq!(hosts(&route(conf.split("Host *").next().unwrap(), "other").unwrap()), ["me@other:22"]);
        // Keywords and patterns are case-insensitive; !pattern excludes
        let r = route("HOST *.internal !secret.internal\n USER ops\n", "DB.internal").unwrap();
        assert_eq!(r[0].user, "ops");
        assert_eq!(route("Host *.internal !secret.internal\n User ops\n", "secret.internal").unwrap()[0].user, "me");
    }

    /// The data source's own port and user win over the config's
    #[test]
    fn explicit_port_and_user_win() {
        let c = SshConfigFile::parse("Host b\n HostName b.example\n User ops\n Port 2222\n", &home()).unwrap();
        let r = c.route("b", Some(2022), Some("root"), &home(), "me").unwrap();
        assert_eq!(hosts(&r), ["root@b.example:2022"]);
        // An empty user is no user
        assert_eq!(c.route("b", None, Some(" "), &home(), "me").unwrap()[0].user, "ops");
    }

    #[test]
    fn proxy_jump_chains_resolve_each_hop() {
        let conf = "Host db-box\n HostName 10.0.0.5\n ProxyJump jump2\nHost jump2\n HostName j2.example\n User j2u\n ProxyJump jump1\nHost jump1\n HostName j1.example\n Port 2201\n";
        assert_eq!(hosts(&route(conf, "db-box").unwrap()), ["me@j1.example:2201", "j2u@j2.example:22", "me@10.0.0.5:22"]);
        // A comma list, user@host:port, ssh:// and an IPv6 address
        let r = route("Host t\n ProxyJump a@h1:2201,ssh://b@h2,[fd00::1]:2222\n", "t").unwrap();
        assert_eq!(hosts(&r), ["a@h1:2201", "b@h2:22", "me@fd00::1:2222", "me@t:22"]);
        // none turns it off
        assert_eq!(route("Host t\n ProxyJump none\nHost *\n ProxyJump x\n", "t").unwrap().len(), 1);
        // A loop is refused, not followed forever
        let e = route("Host a\n ProxyJump b\nHost b\n ProxyJump a\n", "a").unwrap_err().to_string();
        assert!(e.contains("loop"), "{e}");
        assert!(route("Host t\n ProxyJump h:99999\n", "t").is_err());
    }

    #[test]
    fn identity_files_expand_home_and_tokens() {
        let r = route("Host b\n HostName b.example\n User ops\n IdentityFile ~/.ssh/b_key\n IdentityFile \"%d/keys/%r@%h\"\n IdentityFile keys/rel\n", "b").unwrap();
        assert_eq!(r[0].identity_files, [PathBuf::from("/home/me/.ssh/b_key"), PathBuf::from("/home/me/keys/ops@b.example"), PathBuf::from("/home/me/keys/rel")]);
        let e = route("Host b\n IdentityFile ~/.ssh/%C\n", "b").unwrap_err().to_string();
        assert!(e.contains("%C") && e.contains("config:2"), "{e}");
        // IdentityAgent
        assert_eq!(route("Host b\n IdentityAgent none\n", "b").unwrap()[0].agent, Agent::Off);
        assert_eq!(route("Host b\n IdentityAgent ~/agent.sock\n", "b").unwrap()[0].agent, Agent::Socket("/home/me/agent.sock".into()));
        assert_eq!(route("Host b\n IdentityAgent SSH_AUTH_SOCK\n", "b").unwrap()[0].agent, Agent::Env);
    }

    /// No shell: ProxyCommand is refused (with what to do instead), except `ssh -W %h:%p jump`, which is ProxyJump
    #[test]
    fn proxy_command_is_refused_except_ssh_w() {
        let e = route("Host b\n ProxyCommand nc -X 5 -x proxy:1080 %h %p\n", "b").unwrap_err().to_string();
        assert!(e.contains("ProxyCommand isn't supported") && e.contains("ProxyJump"), "{e}");
        assert!(route("Host b\n ProxyCommand ssh -W %h:%p bastion; rm -rf ~\n", "b").is_err());
        assert!(route("Host b\n ProxyCommand ssh -o ProxyCommand=evil -W %h:%p bastion\n", "b").is_err());
        let r = route("Host b\n ProxyCommand /usr/bin/ssh -q -W %h:%p ops@bastion\n", "b").unwrap();
        assert_eq!(hosts(&r), ["ops@bastion:22", "me@b:22"]);
        assert_eq!(route("Host b\n ProxyCommand none\nHost *\n ProxyCommand nc %h %p\n", "b").unwrap().len(), 1);
    }

    #[test]
    fn bad_values_are_refused() {
        assert!(route("Host b\n HostName -oProxyCommand=evil\n", "b").is_err());
        assert!(route("Host b\n HostName a b\n", "b").is_err());
        assert!(route("Host b\n User -l\n", "b").is_err());
        assert!(route("Host b\n Port ssh\n", "b").is_err());
        assert!(route("", "bad host").is_err());
        // HostName %h.example: the alias in the name
        assert_eq!(route("Host *\n HostName %h.corp.example\n", "db1").unwrap()[0].host, "db1.corp.example");
    }

    /// Match blocks are skipped up to the next Host line; Include reads other files (relative to ~/.ssh, * in the name)
    #[test]
    fn match_is_skipped_and_include_is_read() {
        assert_eq!(route("Match host b\n User wrong\nHost b\n Port 2\n", "b").unwrap()[0].user, "me");
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join(".ssh/conf.d")).unwrap();
        std::fs::write(home.join(".ssh/conf.d/10-a.conf"), "Host a\n HostName a.example\n").unwrap();
        std::fs::write(home.join(".ssh/conf.d/20-b.conf"), "Host b\n HostName b.example\n").unwrap();
        std::fs::write(home.join(".ssh/config"), "Include conf.d/*.conf\nHost a\n HostName later.example\n").unwrap();
        let c = SshConfigFile::read(&home.join(".ssh/config"), home).unwrap();
        assert_eq!(c.route("a", None, None, home, "me").unwrap()[0].host, "a.example");
        assert_eq!(c.route("b", None, None, home, "me").unwrap()[0].host, "b.example");
        // A file that includes itself stops
        std::fs::write(home.join(".ssh/loop"), "Include ~/.ssh/loop\n").unwrap();
        assert!(SshConfigFile::read(&home.join(".ssh/loop"), home).is_err());
        // Missing file: empty config; default keys only when they exist
        std::fs::write(home.join(".ssh/id_ed25519"), "").unwrap();
        let c = SshConfigFile::read(&home.join(".ssh/none"), home).unwrap();
        assert_eq!(c.route("x", None, None, home, "me").unwrap()[0].identity_files, [home.join(".ssh/id_ed25519")]);
    }

    #[test]
    fn glob_matches_like_openssh() {
        assert!(glob("*", "anything"));
        assert!(glob("db-?.example", "db-1.example"));
        assert!(!glob("db-?.example", "db-12.example"));
        assert!(glob("*.example*", "a.example.com"));
        assert!(!glob("a*b", "acd"));
    }
}
