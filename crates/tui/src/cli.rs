//! Command-line arguments. Without a URL (or --env) qurry-tui starts as always; with one it also opens a temporary data source
//! (qurry_core::dsn) that is never saved
use qurry_core::store::{DbKind, SshAuth, SshConfig};
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub help: bool,
    pub version: bool,
    pub update: bool,
    pub no_update_check: bool,
    pub config_dir: Option<PathBuf>,
    pub start: Option<Start>,
}

/// A temporary data source to open
#[derive(Debug, PartialEq)]
pub struct Start {
    /// None = DATABASE_URL (--env)
    pub url: Option<String>,
    pub read_only: bool,
    pub incident: bool,
    /// --ssh [user@]host[:port] (a ~/.ssh/config Host alias works too)
    pub ssh: Option<String>,
}

pub const USAGE: &str = "qurry-tui [--config-dir DIR] [--no-update-check]
       qurry-tui [OPTIONS] postgres://[user[:password]@]host[:port]/db[?sslmode=…]
       qurry-tui [OPTIONS] mysql://[user[:password]@]host[:port]/db[?ssl-mode=…]
       qurry-tui [OPTIONS] redis://[[user]:password@]host[:port]/n      (rediss:// for TLS)
       qurry-tui [OPTIONS] --env          the URL in DATABASE_URL
       qurry-tui --update     download and install the newest release of the update channel (stable, or beta: ⌃k › Update Channel…)

A URL opens a temporary data source and connects: it isn't saved, isn't shared with the Qurry app or MCP, and is gone on exit.
Parts the URL leaves out come from PGHOST PGPORT PGUSER PGDATABASE PGPASSWORD PGSSLMODE (and ~/.pgpass, mode 0600),
MYSQL_PWD or REDISCLI_AUTH. Put the password there rather than in the URL: other users of this computer can see the command line.
  --read-only        refuse statements that change data (as a Read-only data source)
  --incident         open straight into Database Admin › Sessions (PostgreSQL, MySQL)
  --ssh DEST         through an SSH tunnel to [user@]host[:port] or a ~/.ssh/config Host (ssh-agent, IdentityFile, ProxyJump)";

pub fn parse(args: &[String]) -> Result<Cli, String> {
    let mut c = Cli::default();
    let (mut url, mut env, mut read_only, mut incident, mut ssh) = (None, false, false, false, None);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => c.help = true,
            "-V" | "--version" => c.version = true,
            "--update" => c.update = true,
            "--no-update-check" => c.no_update_check = true,
            "--config-dir" => c.config_dir = Some(it.next().ok_or("--config-dir needs a folder")?.into()),
            "--read-only" => read_only = true,
            "--incident" => incident = true,
            "--env" => env = true,
            "--ssh" => ssh = Some(it.next().ok_or("--ssh needs [user@]host[:port] or a ~/.ssh/config Host")?.clone()),
            u if u.contains("://") && url.is_none() => url = Some(u.to_string()),
            // A URL's text could be a password: never echo it
            u if u.contains("://") => return Err("Give one URL".into()),
            other => return Err(format!("Unknown argument \"{}\"", other.escape_debug())),
        }
    }
    if url.is_some() && env {
        return Err("Give a URL or --env, not both".into());
    }
    if url.is_some() || env {
        c.start = Some(Start { url, read_only, incident, ssh });
    } else if let Some(f) = [(read_only, "--read-only"), (incident, "--incident"), (ssh.is_some(), "--ssh")].iter().find(|f| f.0).map(|f| f.1) {
        return Err(format!("{f} is for a temporary connection: qurry-tui {f} postgres://user@host/db (or --env)"));
    }
    Ok(c)
}

/// The data source of `s` (with the process environment and ~/.pgpass), and what to tell the user. Err: why it can't be opened
pub fn temporary(s: &Start) -> Result<qurry_core::dsn::Temporary, String> {
    let env = qurry_core::dsn::Env::process();
    let mut t = match &s.url {
        Some(u) => qurry_core::dsn::from_url(u, s.read_only, &env),
        None => qurry_core::dsn::from_database_url(&env, s.read_only),
    }
    .map_err(|e| e.to_string())?;
    if let Some(dest) = &s.ssh {
        let (user, host, port) = qurry_core::ssh_config::parse_destination(dest, "--ssh").map_err(|e| e.to_string())?;
        // No key file and no password: ssh-agent, then the ~/.ssh/config IdentityFile keys (core::tunnel)
        t.config.ssh = Some(SshConfig { enabled: true, host, port: port.unwrap_or(22), user: user.unwrap_or_default(), auth: SshAuth::Key, key_path: String::new() });
        t.config.name = format!("{} via ssh {dest}", t.config.name);
    }
    if s.incident && !matches!(t.config.kind, DbKind::Postgres | DbKind::Mysql) {
        return Err("--incident opens Database Admin › Sessions, which PostgreSQL and MySQL have (Redis doesn't)".into());
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        std::iter::once("qurry-tui").chain(a.iter().copied()).map(String::from).collect()
    }

    /// The forms that existed before URLs behave as they did
    #[test]
    fn old_forms_are_unchanged() {
        assert_eq!(parse(&args(&[])).unwrap(), Cli::default());
        let c = parse(&args(&["--config-dir", "/tmp/x", "--no-update-check"])).unwrap();
        assert_eq!((c.config_dir, c.no_update_check, c.start), (Some("/tmp/x".into()), true, None));
        assert!(parse(&args(&["--update"])).unwrap().update);
        assert!(parse(&args(&["--version"])).unwrap().version && parse(&args(&["-V"])).unwrap().version);
        assert!(parse(&args(&["-h"])).unwrap().help);
    }

    #[test]
    fn url_and_options() {
        let c = parse(&args(&["--read-only", "postgres://u@h/d", "--incident", "--ssh", "bastion", "--config-dir", "/x"])).unwrap();
        assert_eq!(c.start, Some(Start { url: Some("postgres://u@h/d".into()), read_only: true, incident: true, ssh: Some("bastion".into()) }));
        assert_eq!(parse(&args(&["--env"])).unwrap().start.unwrap().url, None);
        // Options without a URL, a typo, two URLs: refused (a misspelled --read-only must not open a writable connection)
        assert!(parse(&args(&["--read-only"])).unwrap_err().contains("temporary connection"));
        assert!(parse(&args(&["--readonly", "postgres://h/d"])).unwrap_err().contains("--readonly"));
        let e = parse(&args(&["postgres://u:secret@h/d", "mysql://u:secret2@h/d"])).unwrap_err();
        assert!(!e.contains("secret"), "{e}");
        assert!(parse(&args(&["--env", "postgres://h/d"])).is_err());
        assert!(parse(&args(&["--ssh"])).is_err());
    }

    #[test]
    fn ssh_and_incident_are_checked() {
        let start = |url: &str, ssh: Option<&str>, incident: bool| Start { url: Some(url.into()), read_only: false, incident, ssh: ssh.map(String::from) };
        let t = temporary(&start("postgres://u@db.internal/app", Some("ops@bastion:2222"), true)).unwrap();
        let ssh = t.config.ssh.unwrap();
        assert_eq!((ssh.user.as_str(), ssh.host.as_str(), ssh.port, ssh.key_path.as_str()), ("ops", "bastion", 2222, ""));
        assert!(t.config.name.ends_with("via ssh ops@bastion:2222"));
        assert!(temporary(&start("postgres://u@h/d", Some("-oProxyCommand=x"), false)).is_err());
        assert!(temporary(&start("redis://h", None, true)).unwrap_err().contains("--incident"));
        assert!(temporary(&start("ftp://h", None, false)).unwrap_err().contains("Unknown scheme"));
    }
}
