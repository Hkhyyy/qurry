//! Password command (#177): a program whose output is the DB password, run at each login (an RDS / Cloud SQL IAM token).
//! Never through a shell: the field is split into words (shlex), the placeholders are replaced inside each word, and the first
//! word is the program, by its full path. Its output goes nowhere but the login (and AppState's in-memory reuse cache): never
//! into a log line or an error, which quote at most its exit code and the start of its standard error
use crate::error::{AppError, AppResult};
use crate::store::ConnectionConfig;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};
use tokio::io::AsyncReadExt;

/// How long the command may run before it is stopped (an `aws` CLI signing in again, a hung helper)
pub(crate) const PASSWORD_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// A run of a command kept for reuse: its words, when it started (by the wall clock, which a token's life follows: an Instant
/// stops while the Mac sleeps), what it printed
pub(crate) type Ran = (Vec<String>, SystemTime, String);

/// How much of the command's standard error an error quotes (the rest is read and dropped)
const STDERR_SHOWN: usize = 4096;

/// The most a password command may print (an RDS IAM token is about 1 KB): a command that streams is stopped there
const STDOUT_MAX: usize = 64 * 1024;

/// The command's words with the placeholders replaced: %HOSTNAME% %PORT% %USERNAME% %DATABASE% are the data source's own (its
/// host and port, not a tunnel's local one). Replaced after splitting, so a value with spaces stays one argument
pub(crate) fn argv(cfg: &ConnectionConfig) -> AppResult<Vec<String>> {
    let words = shlex::split(cfg.password_command.trim()).ok_or_else(|| AppError::Msg("The password command has a quote that isn't closed.".into()))?;
    let Some(program) = words.first() else { return Err(AppError::Msg("The password command is empty.".into())) };
    if !std::path::Path::new(program).is_absolute() {
        return Err(AppError::Msg(format!(
            "Use the full path of the password command's program, not \"{program}\" (an app started from Finder doesn't get the shell's PATH): /opt/homebrew/bin/aws …"
        )));
    }
    let port = cfg.port.to_string();
    let fill = |w: &str| w.replace("%HOSTNAME%", &cfg.host).replace("%PORT%", &port).replace("%USERNAME%", &cfg.user).replace("%DATABASE%", &cfg.database);
    Ok(std::iter::once(program.clone()).chain(words[1..].iter().map(|w| fill(w))).collect())
}

/// Runs `cfg`'s password command (argv) and returns what it printed, trimmed. Stopped after `timeout`
pub(crate) async fn run(cfg: &ConnectionConfig, timeout: Duration) -> AppResult<String> {
    let argv = argv(cfg)?;
    let id = if cfg.id.is_empty() { "(unsaved)" } else { &cfg.id };
    let started = Instant::now();
    let ms = || started.elapsed().as_millis();
    let mut child = tokio::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            crate::log!("[qurry] password command of {id} couldn't start after {} ms", ms());
            AppError::Msg(format!("The password command {} couldn't start: {e}", argv[0]))
        })?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let done = tokio::time::timeout(timeout, async { tokio::join!(read_upto(out, STDOUT_MAX + 1, false), read_upto(err, STDERR_SHOWN, true), child.wait()) }).await;
    let (stdout, stderr, status) = match done {
        Ok((o, e, s)) => (o, e, s.map_err(|e| AppError::Msg(format!("The password command couldn't be waited for: {e}")))?),
        Err(_) => {
            // Killed and reaped before the error, so nothing of it is left running
            let _ = child.kill().await;
            crate::log!("[qurry] password command of {id} timed out after {} ms", ms());
            return Err(AppError::Msg(format!(
                "The password command didn't finish in {} s and was stopped. Run it in a terminal to see what it waits for (an expired AWS SSO session: aws sso login).",
                timeout.as_secs_f32()
            )));
        }
    };
    if stdout.len() > STDOUT_MAX {
        crate::log!("[qurry] password command of {id} printed more than {} KB after {} ms", STDOUT_MAX / 1024, ms());
        return Err(AppError::Msg(format!("The password command printed more than {} KB: it must print only the password.", STDOUT_MAX / 1024)));
    }
    if !status.success() {
        let code = status.code().map_or_else(|| "killed by a signal".to_string(), |c| format!("exit {c}"));
        crate::log!("[qurry] password command of {id} {} after {} ms", status.code().map_or_else(|| "was killed by a signal".to_string(), |c| format!("exit code {c}")), ms());
        let shown = String::from_utf8_lossy(&stderr).trim().to_string();
        return Err(AppError::Msg(format!("Password command failed ({code}): {}", if shown.is_empty() { "(nothing on standard error)" } else { &shown })));
    }
    let Ok(password) = String::from_utf8(stdout) else {
        crate::log!("[qurry] password command of {id} printed no text after {} ms", ms());
        return Err(AppError::Msg("The password command printed something that isn't text.".into()));
    };
    let password = password.trim().to_string();
    if password.is_empty() {
        crate::log!("[qurry] password command of {id} printed nothing after {} ms", ms());
        return Err(AppError::Msg("The password command printed nothing: it must print the password on standard output.".into()));
    }
    crate::log!("[qurry] password command of {id} ok in {} ms", ms());
    Ok(password)
}

impl crate::app::AppState {
    /// The DB password a login of `cfg` sends: its password command's output when it has one (the saved password, if any, isn't
    /// read then, and stays in secrets.json untouched), else the saved password. A file database (SQLite, DuckDB) sends none, so
    /// its command is ignored
    pub(crate) async fn login_password(&self, cfg: &ConnectionConfig) -> AppResult<Option<String>> {
        if cfg.password_command.trim().is_empty() || cfg.kind.is_file() {
            return self.saved_secret(&cfg.password_key(), cfg.password_saved);
        }
        self.mcp_may_log_in(&cfg.id)?;
        self.command_password(cfg, false).await.map(Some)
    }

    /// The password command's output: the one kept from its last run while that is younger than password_reuse_secs and the
    /// command's words (with the placeholders replaced) are the same; `fresh`, or otherwise, it runs again
    pub(crate) async fn command_password(&self, cfg: &ConnectionConfig, fresh: bool) -> AppResult<String> {
        let words = argv(cfg)?;
        if let Some(reuse) = cfg.password_reuse_secs.filter(|_| !fresh).map(|s| Duration::from_secs(s.into()))
            && let Some((w, at, password)) = self.command_passwords.lock().unwrap().get(&cfg.id)
            && *w == words
            // A clock set back counts as expired
            && at.elapsed().is_ok_and(|e| e < reuse)
        {
            return Ok(password.clone());
        }
        let started = SystemTime::now();
        let password = run(cfg, PASSWORD_COMMAND_TIMEOUT).await?;
        // Defense in depth: no log line shows it even if a driver's error text quotes it. One hidden word per data source: the
        // token it replaces is shown no more (a new IAM token at every login would grow the list for as long as the app runs)
        crate::logfile::hide_word(&password, "<password>");
        // A form that isn't saved has no id to keep it under
        if !cfg.id.is_empty() {
            let mut kept = self.command_passwords.lock().unwrap();
            if let Some((_, _, old)) = kept.insert(cfg.id.clone(), (words, started, password.clone()))
                && old != password
                && !kept.values().any(|(_, _, p)| *p == old)
            {
                crate::logfile::unhide_word(&old, "<password>");
            }
        }
        Ok(password)
    }

    /// Whether the password command's output kept for `cfg` is from a run that started before `since`: a login with it that the
    /// server refused may be with a token that has expired since (open_pool runs the command again then)
    pub(crate) fn command_password_older(&self, cfg: &ConnectionConfig, since: SystemTime) -> bool {
        !cfg.password_command.trim().is_empty() && self.command_passwords.lock().unwrap().get(&cfg.id).is_some_and(|(_, at, _)| *at < since)
    }
}

/// What a pipe of the command gives until it closes, up to `max` bytes. `drain`: the rest is read and dropped (so the command
/// isn't stopped by a full pipe); otherwise the pipe is closed there
async fn read_upto(pipe: Option<impl tokio::io::AsyncRead + Unpin>, max: usize, drain: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut p) = pipe {
        let _ = (&mut p).take(max as u64).read_to_end(&mut buf).await;
        if drain {
            let _ = tokio::io::copy(&mut p, &mut tokio::io::sink()).await;
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(command: &str) -> ConnectionConfig {
        let mut c = crate::testenv::parse_pg("db.example.test:6543:iam_user:orders").cfg;
        c.password_command = command.into();
        c
    }

    const T: Duration = Duration::from_secs(10);

    #[test]
    fn placeholders_are_replaced_inside_each_argument() {
        let mut c = cfg(r#"/bin/echo %USERNAME%@%HOSTNAME%:%PORT%/%DATABASE% "a b %DATABASE%" --region ap-northeast-2"#);
        c.database = "my db".into();
        let a = argv(&c).unwrap();
        assert_eq!(a, ["/bin/echo", "iam_user@db.example.test:6543/my db", "a b my db", "--region", "ap-northeast-2"]);
    }

    #[tokio::test]
    async fn the_output_is_the_password() {
        let c = cfg(r#"/bin/echo %USERNAME%@%HOSTNAME%:%PORT%/%DATABASE% "two  spaces""#);
        assert_eq!(run(&c, T).await.unwrap(), "iam_user@db.example.test:6543/orders two  spaces");
        // A value with spaces stays one argument: printf ends each argument with |
        let mut c = cfg(r#"/usr/bin/printf "%s|" %DATABASE% x"#);
        c.database = "a b".into();
        assert_eq!(run(&c, T).await.unwrap(), "a b|x|");
    }

    #[tokio::test]
    async fn a_program_without_its_full_path_is_refused() {
        for c in ["aws rds generate-db-auth-token", "./token.sh", "bin/aws"] {
            let e = run(&cfg(c), T).await.unwrap_err().to_string();
            assert!(e.contains("full path") && e.contains("/opt/homebrew/bin/aws"), "{e}");
        }
        assert!(run(&cfg("/bin/echo 'open"), T).await.unwrap_err().to_string().contains("quote"));
    }

    #[tokio::test]
    async fn printing_nothing_is_an_error() {
        let e = run(&cfg("/usr/bin/true"), T).await.unwrap_err().to_string();
        assert!(e.contains("printed nothing"), "{e}");
    }

    #[tokio::test]
    async fn a_failure_shows_its_exit_code_and_stderr_never_its_output() {
        let e = run(&cfg("/bin/sh -c 'echo oops >&2; echo s3cret; exit 3'"), T).await.unwrap_err().to_string();
        assert!(e.contains("exit 3") && e.contains("oops"), "{e}");
        assert!(!e.contains("s3cret"), "{e}");
        let e = run(&cfg("/no/such/program"), T).await.unwrap_err().to_string();
        assert!(e.contains("couldn't start"), "{e}");
    }

    /// A script that notes each run in `runs` and prints a token
    fn counting(dir: &std::path::Path) -> (String, std::path::PathBuf) {
        let (script, runs) = (dir.join("token.sh"), dir.join("runs"));
        std::fs::write(&script, format!("echo run >> '{}'\necho tok-$1\n", runs.display())).unwrap();
        (format!("/bin/sh {} %HOSTNAME%", script.display()), runs)
    }

    fn runs(path: &std::path::Path) -> usize {
        std::fs::read_to_string(path).map_or(0, |s| s.lines().count())
    }

    #[tokio::test]
    async fn the_output_is_reused_only_within_its_time_and_for_the_same_command() {
        let t = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let (command, n) = counting(t.path());
        let mut c = cfg(&command);
        c.password_reuse_secs = Some(600);
        assert_eq!(st.login_password(&c).await.unwrap().as_deref(), Some("tok-db.example.test"));
        assert_eq!(st.login_password(&c).await.unwrap().as_deref(), Some("tok-db.example.test"));
        assert_eq!(runs(&n), 1, "reused");
        c.host = "other.example.test".into();
        assert_eq!(st.login_password(&c).await.unwrap().as_deref(), Some("tok-other.example.test"));
        assert_eq!(runs(&n), 2, "another host runs it again");
        st.command_password(&c, true).await.unwrap();
        assert_eq!(runs(&n), 3, "fresh");
        c.password_reuse_secs = None;
        st.login_password(&c).await.unwrap();
        st.login_password(&c).await.unwrap();
        assert_eq!(runs(&n), 5, "blank reuse runs it at every login");
        // An unsaved form keeps nothing
        let mut form = c.clone();
        (form.id, form.password_reuse_secs) = (String::new(), Some(600));
        st.login_password(&form).await.unwrap();
        st.login_password(&form).await.unwrap();
        assert_eq!(runs(&n), 7);
    }

    /// Reuse ages by the wall clock: a Mac that slept past the token's life runs the command again (an Instant would not have
    /// moved), and so does a clock set back
    #[tokio::test]
    async fn reuse_counts_time_the_mac_slept() {
        let t = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let (command, n) = counting(t.path());
        let mut c = cfg(&command);
        c.password_reuse_secs = Some(600);
        let shift = |by: i64| {
            if let Some((_, at, _)) = st.command_passwords.lock().unwrap().get_mut(&c.id) {
                *at = if by < 0 { *at - Duration::from_secs(by.unsigned_abs()) } else { *at + Duration::from_secs(by as u64) };
            }
        };
        st.login_password(&c).await.unwrap();
        shift(-3600);
        st.login_password(&c).await.unwrap();
        assert_eq!(runs(&n), 2, "an hour asleep");
        shift(120);
        st.login_password(&c).await.unwrap();
        assert_eq!(runs(&n), 3, "the clock was set back");
        st.login_password(&c).await.unwrap();
        assert_eq!(runs(&n), 3, "reused");
        assert!(st.command_password_older(&c, SystemTime::now() + Duration::from_secs(1)));
        assert!(!st.command_password_older(&c, SystemTime::now() - Duration::from_secs(60)));
        c.password_command.clear();
        assert!(!st.command_password_older(&c, SystemTime::now() + Duration::from_secs(1)), "no command: nothing to run again");
    }

    /// A command that prints far more than a password is stopped; a long standard error is read past what is shown
    #[tokio::test]
    async fn output_is_limited() {
        let e = run(&cfg("/bin/cat /dev/zero"), T).await.unwrap_err().to_string();
        assert!(e.contains("more than 64 KB"), "{e}");
        let c = cfg("/bin/sh -c '/usr/bin/yes w | /usr/bin/head -c 300000 >&2; echo tok'");
        assert_eq!(run(&c, T).await.unwrap(), "tok");
        let c = cfg("/bin/sh -c '/usr/bin/yes w | /usr/bin/head -c 300000 >&2; exit 1'");
        let e = run(&c, T).await.unwrap_err().to_string();
        assert!(e.len() < STDERR_SHOWN + 100 && e.contains("w\nw"), "{}", e.len());
    }

    /// A file database sends no password: its command (the TUI form shows the row) never runs
    #[tokio::test]
    async fn a_file_database_ignores_its_command() {
        let t = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let mut c = cfg("/no/such/program");
        c.kind = crate::store::DbKind::Sqlite;
        assert_eq!(st.login_password(&c).await.unwrap(), None);
    }

    /// With a command the saved password is neither read nor removed
    #[tokio::test]
    async fn a_command_wins_over_the_saved_password() {
        let t = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let saved = st.save_connection(cfg(""), Some("saved-pw".into()), None).await.unwrap();
        assert_eq!(st.login_password(&saved).await.unwrap().as_deref(), Some("saved-pw"));
        let mut c = saved.clone();
        c.password_command = "/bin/echo from-command".into();
        let c = st.save_connection(c, None, None).await.unwrap();
        assert_eq!(st.login_password(&c).await.unwrap().as_deref(), Some("from-command"));
        assert!(c.password_saved && st.secret(&c.password_key()).unwrap().as_deref() == Some("saved-pw"), "kept");
        // A new host: no password to enter while the command is set, so the old one is forgotten instead of refusing the save
        let mut moved = c.clone();
        moved.host = "moved.example.test".into();
        let moved = st.save_connection(moved, None, None).await.unwrap();
        assert_eq!(st.login_password(&moved).await.unwrap().as_deref(), Some("from-command"));
        assert!(!moved.password_saved && !st.secret_file.contains(&moved.password_key()).unwrap());
        // Without a command the host change still asks for the password
        let mut plain = saved.clone();
        plain.host = "third.example.test".into();
        st.save_connection(saved.clone(), Some("again".into()), None).await.unwrap();
        assert!(st.save_connection(plain, None, None).await.unwrap_err().to_string().contains("enter the password again"));
    }

    /// A source whose password comes from its command connects and reads, isn't exported with it, and after the server changed
    /// the password and dropped its connections (an expired IAM token) its pool and a new console run the command again and log in:
    /// QURRY_TEST_PG=… QURRY_TEST_MY=… cargo test -p qurry-core password_command -- --ignored
    async fn a_source_logs_in_with_its_command(kind: crate::store::DbKind) {
        let Some(server) = crate::testenv::server(kind) else { return };
        let pg = kind == crate::store::DbKind::Postgres;
        let t = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(t.path().join("config"))).unwrap();
        let mut admin = server.cfg.clone();
        admin.id = String::new();
        let admin = st.save_connection(admin, server.password.clone(), None).await.unwrap().id;
        let sql = async |s: &str| st.run_query(admin.clone(), s.to_string(), Some(100), None).await;
        let _ = sql(if pg { "drop role if exists qurry_pwcmd" } else { "drop user if exists 'qurry_pwcmd'@'%'" }).await;
        sql(if pg { "create role qurry_pwcmd login password 'Pw-one-1'" } else { "create user 'qurry_pwcmd'@'%' identified by 'Pw-one-1'" }).await.unwrap();
        let (password, script, runs_file) = (t.path().join("pw"), t.path().join("token.sh"), t.path().join("runs"));
        std::fs::write(&password, "Pw-one-1\n").unwrap();
        std::fs::write(&script, format!("echo run >> '{}'\ncat '{}'\n", runs_file.display(), password.display())).unwrap();
        let mut c = server.cfg.clone();
        (c.id, c.user, c.password_command) = (String::new(), "qurry_pwcmd".into(), format!("/bin/sh {}", script.display()));
        c.database = if pg { "postgres".into() } else { String::new() };
        let id = st.save_connection(c, None, None).await.unwrap().id;
        let r = st.run_read_only(id.clone(), "select 1".into(), 10).await.unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(runs(&runs_file), 1);

        let file = t.path().join("export.json");
        st.export_connections(file.display().to_string()).await.unwrap();
        let other = crate::open_state(Some(t.path().join("other"))).unwrap();
        let plan = other.import_plan(&file, t.path()).unwrap();
        other.import_apply(&plan, &[]).await.unwrap();
        assert!(other.store.list().unwrap().iter().all(|c| c.password_command.is_empty()));

        sql(if pg { "alter role qurry_pwcmd password 'Pw-two-2'" } else { "alter user 'qurry_pwcmd'@'%' identified by 'Pw-two-2'" }).await.unwrap();
        std::fs::write(&password, "Pw-two-2\n").unwrap();
        if pg {
            sql("select pg_terminate_backend(pid) from pg_stat_activity where usename = 'qurry_pwcmd'").await.unwrap();
        } else {
            let ids = sql("select id from information_schema.processlist where user = 'qurry_pwcmd'").await.unwrap();
            for row in ids.rows {
                let _ = sql(&format!("kill {}", row[0].as_u64().map_or_else(|| row[0].as_str().unwrap_or("0").to_string(), |n| n.to_string()))).await;
            }
        }
        let r = st.run_read_only(id.clone(), "select 1".into(), 10).await.expect("the pool logs in with the new password");
        assert_eq!(r.rows.len(), 1);
        assert_eq!(runs(&runs_file), 2, "the command ran again");
        st.run_query(id.clone(), "select 1".into(), Some(10), Some("tab".into())).await.expect("a console opens on it");
        st.close_session("tab".into()).await.unwrap();
        st.disconnect(id.clone()).await.unwrap();

        // With Password reuse the output is kept past a Disconnect: when the server no longer takes it (the token expired while no
        // pool was open), opening the pool runs the command again
        let mut c = st.source(&id).unwrap();
        c.password_reuse_secs = Some(600);
        st.save_connection(c, None, None).await.unwrap();
        st.run_read_only(id.clone(), "select 1".into(), 10).await.unwrap();
        let before = runs(&runs_file);
        st.disconnect(id.clone()).await.unwrap();
        sql(if pg { "alter role qurry_pwcmd password 'Pw-three-3'" } else { "alter user 'qurry_pwcmd'@'%' identified by 'Pw-three-3'" }).await.unwrap();
        std::fs::write(&password, "Pw-three-3\n").unwrap();
        st.run_read_only(id.clone(), "select 1".into(), 10).await.expect("the pool opens with a new output");
        assert_eq!(runs(&runs_file), before + 1, "the command ran again");
        st.run_read_only(id.clone(), "select 1".into(), 10).await.unwrap();
        assert_eq!(runs(&runs_file), before + 1, "and its output is reused");
        st.disconnect(id).await.unwrap();
        sql(if pg { "drop role qurry_pwcmd" } else { "drop user 'qurry_pwcmd'@'%'" }).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn password_command_logs_in_pg() {
        a_source_logs_in_with_its_command(crate::store::DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn password_command_logs_in_my() {
        a_source_logs_in_with_its_command(crate::store::DbKind::Mysql).await;
    }

    #[tokio::test]
    async fn a_command_that_runs_too_long_is_stopped() {
        let started = Instant::now();
        let pid_file = tempfile::NamedTempFile::new().unwrap();
        let c = cfg(&format!("/bin/sh -c 'echo $$ > {}; exec /bin/sleep 60'", pid_file.path().display()));
        let e = run(&c, Duration::from_millis(500)).await.unwrap_err().to_string();
        assert!(e.contains("didn't finish") && started.elapsed() < Duration::from_secs(5), "{e}");
        let pid = std::fs::read_to_string(pid_file.path()).unwrap().trim().to_string();
        let alive = std::process::Command::new("/bin/ps").args(["-p", &pid]).stdout(Stdio::null()).status().unwrap().success();
        assert!(!alive, "the command ({pid}) is gone");
        let e = run(&cfg("/bin/sleep 60"), Duration::from_millis(200)).await.unwrap_err().to_string();
        assert!(e.contains("didn't finish"), "{e}");
    }
}
