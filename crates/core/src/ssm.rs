//! AWS SSM Session Manager tunnel: runs `aws ssm start-session` with the AWS-StartPortForwardingSessionToRemoteHost document on a free
//! local port, waits until that port accepts, and stops the CLI (and the Session Manager plugin it starts) when the Tunnel is dropped.
//! Qurry stores no AWS secrets: the AWS CLI uses the user's own config, SSO login or environment. Arguments go as a vector (no shell)
//! and are checked first, so a data source can't add CLI options or document parameters.
use crate::error::{AppError, AppResult};
use crate::store::SsmConfig;
use crate::tunnel::Tunnel;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};

/// Starting a session (credentials, StartSession, the plugin's websocket) usually takes a few seconds
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALL_CLI: &str = "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html";
const INSTALL_PLUGIN: &str = "https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html";
/// How much of the CLI's latest output is kept for the error message
const TAIL: usize = 4096;

/// The running `aws ssm start-session`. Dropping it kills the CLI and everything it started
pub struct Process {
    child: Child,
}

impl Drop for Process {
    fn drop(&mut self) {
        // The CLI runs session-manager-plugin as its own child, which holds the port and the session: stop the whole process group.
        // id() is None once the CLI was reaped (then its pid may belong to someone else)
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        }
        // Elsewhere kill_on_drop stops the CLI only
    }
}

fn cli_missing() -> AppError {
    AppError::Msg(format!("AWS CLI not found. Install AWS CLI v2 ({INSTALL_CLI}, or brew install awscli), then try again."))
}

/// Opens the tunnel to `host:port` as seen from the target instance (the data source's host/port)
pub async fn open(ssm: &SsmConfig, host: &str, port: u16) -> AppResult<Tunnel> {
    // A GUI app started from the Dock has a short PATH: look where the AWS installers put the CLI and the plugin too
    let aws = crate::backup::tool_paths("aws").into_iter().next().ok_or_else(cli_missing)?;
    let path = std::env::join_paths(crate::backup::tool_dirs()).ok();
    open_with(&aws, path, ssm, host, port, READY_TIMEOUT).await
}

async fn open_with(aws: &Path, path: Option<OsString>, ssm: &SsmConfig, host: &str, port: u16, timeout: Duration) -> AppResult<Tunnel> {
    let local = free_port()?;
    let mut cmd = Command::new(aws);
    cmd.args(args(ssm, host, port, local)?).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => cli_missing(),
        _ => AppError::Msg(format!("Couldn't run the AWS CLI ({}): {e}", aws.display())),
    })?;
    // Read the output for as long as the CLI runs (the plugin writes a line per connection; a full pipe would stop it)
    let tail = Arc::new(Mutex::new(String::new()));
    let readers = [drain(child.stdout.take(), tail.clone()), drain(child.stderr.take(), tail.clone())];
    let mut child = Process { child };
    let deadline = Instant::now() + timeout;
    loop {
        if child.child.try_wait()?.is_some() {
            for r in readers {
                let _ = tokio::time::timeout(Duration::from_secs(1), r).await;
            }
            let out = tail.lock().unwrap().clone();
            return Err(AppError::Msg(explain(&out, ssm)));
        }
        if tokio::net::TcpStream::connect(("127.0.0.1", local)).await.is_ok() {
            return Ok(Tunnel::process(local, child));
        }
        if Instant::now() >= deadline {
            let out = tail.lock().unwrap().clone();
            let last = last_line(&out).map(|l| format!(" Last output: {l}")).unwrap_or_default();
            return Err(AppError::Msg(format!(
                "The AWS SSM session to {} didn't open local port {local} within {}s. Check that the instance is running and connected to Session Manager, and that the region is right.{last}",
                ssm.target.trim(),
                timeout.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn free_port() -> AppResult<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn drain(r: Option<impl AsyncRead + Unpin + Send + 'static>, tail: Arc<Mutex<String>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(r) = r else { return };
        let mut r = BufReader::new(r);
        let mut line = Vec::new();
        while r.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
            let mut t = tail.lock().unwrap();
            t.push_str(&String::from_utf8_lossy(&line));
            if t.len() > TAIL {
                let cut = t.len() - TAIL;
                let cut = (cut..t.len()).find(|i| t.is_char_boundary(*i)).unwrap_or(t.len());
                t.drain(..cut);
            }
            line.clear();
        }
    })
}

/// The CLI arguments. Each value is checked: no shell is involved, but a value could still pass another option or document parameter
fn args(ssm: &SsmConfig, host: &str, port: u16, local: u16) -> AppResult<Vec<String>> {
    let target = ssm.target.trim();
    let hex = |s: &str, n: std::ops::RangeInclusive<usize>| n.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !target.strip_prefix("i-").or_else(|| target.strip_prefix("mi-")).is_some_and(|h| hex(h, 8..=17)) {
        return Err(AppError::Msg(format!("AWS SSM target must be an instance id like i-0123456789abcdef0 (or a managed node mi-…), not {target:?}")));
    }
    let (profile, region) = (ssm.profile.trim(), ssm.region.trim());
    if profile.len() > 64 || profile.starts_with('-') || !profile.chars().all(|c| c.is_ascii_alphanumeric() || "_.-@+".contains(c)) {
        return Err(AppError::Msg(format!("AWS profile {profile:?} has characters Qurry doesn't pass on (use letters, digits and _ . - @ +)")));
    }
    if region.len() > 32 || region.starts_with('-') || !region.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(AppError::Msg(format!("AWS region {region:?} isn't a region name like us-east-1")));
    }
    let host = host.trim();
    if host.is_empty() || host.starts_with('-') || !host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)) {
        return Err(AppError::Msg(format!("With AWS SSM the database host must be a host name or IPv4 address as seen from the instance, not {host:?}")));
    }
    let mut a: Vec<String> = ["ssm", "start-session", "--target", target, "--document-name", "AWS-StartPortForwardingSessionToRemoteHost", "--parameters"].map(String::from).into();
    a.push(format!("host={host},portNumber={port},localPortNumber={local}"));
    for (flag, v) in [("--profile", profile), ("--region", region)] {
        if !v.is_empty() {
            a.extend([flag.to_string(), v.to_string()]);
        }
    }
    Ok(a)
}

fn last_line(out: &str) -> Option<String> {
    out.lines().map(str::trim).rfind(|l| !l.is_empty()).map(|l| l.chars().take(300).collect())
}

/// The CLI's error, as what to do next. Only the CLI's own last line is ever shown, never the environment
fn explain(out: &str, ssm: &SsmConfig) -> String {
    let profile = ssm.profile.trim();
    let login = if profile.is_empty() { "aws sso login".to_string() } else { format!("aws sso login --profile {profile}") };
    let target = ssm.target.trim();
    let low = out.to_lowercase();
    let has = |s: &str| low.contains(&s.to_lowercase());
    if has("SessionManagerPlugin is not found") || has("session-manager-plugin-not-found") {
        format!("The AWS Session Manager plugin is not installed. Install it (macOS: brew install --cask session-manager-plugin; others: {INSTALL_PLUGIN}), then try again.")
    } else if has("AccessDenied") || has("is not authorized to perform") {
        format!("Access denied: your AWS identity may not start a session on {target}. It needs ssm:StartSession on the instance and on the AWS-StartPortForwardingSessionToRemoteHost document.")
    } else if has("TargetNotConnected") {
        format!("{target} is not connected to Session Manager: it is stopped, in another region than the one used, or its SSM agent is offline. Check the instance id and region.")
    } else if has("sso") && (has("expired") || has("sso login") || has("Error loading SSO Token")) {
        format!("Your AWS SSO session has expired or you haven't signed in. Run: {login} — then try again.")
    } else if has("ExpiredToken") || has("security token included in the request is expired") {
        format!("Your AWS credentials have expired. Refresh them (for SSO run: {login}), then try again.")
    } else if has("Unable to locate credentials") {
        "The AWS CLI found no credentials. Sign in (aws configure sso, then aws sso login) or set a profile in the data source, then try again.".to_string()
    } else if has("config profile") && has("could not be found") {
        format!("AWS profile {profile:?} isn't in your AWS config. Check the name (aws configure list-profiles).")
    } else if has("You must specify a region") {
        "No AWS region: set Region in the data source (or a region in the AWS profile).".to_string()
    } else {
        match last_line(out) {
            Some(l) => format!("aws ssm start-session failed: {l}"),
            None => "aws ssm start-session stopped without saying why.".to_string(),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A fake `aws` that prints what the real CLI prints. "ok" starts a fake plugin (a child, like the real CLI) that listens on
    /// localPortNumber and forwards to host:portNumber; it writes its argv and the pids next to itself
    const FAKE: &str = r#"#!/usr/bin/env python3
import json, os, socket, subprocess, sys, threading, time
MODE = "@MODE@"
HERE = os.path.dirname(os.path.abspath(__file__))
def pipe(a, b):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            b.sendall(d)
    except OSError:
        pass
    for s in (a, b):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
if sys.argv[1:2] == ["--plugin"]:
    host, port, local = sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    srv = socket.socket()
    srv.bind(("127.0.0.1", local))
    srv.listen(16)
    print(f"Port {local} opened for sessionId qurry-test-0123456789abcdef.", flush=True)
    print("Waiting for connections...", flush=True)
    while True:
        c, _ = srv.accept()
        print("\nConnection accepted for session [qurry-test-0123456789abcdef]", flush=True)
        try:
            u = socket.create_connection((host, port))
        except OSError:
            c.close()
            continue
        threading.Thread(target=pipe, args=(c, u), daemon=True).start()
        threading.Thread(target=pipe, args=(u, c), daemon=True).start()
args = sys.argv[1:]
with open(os.path.join(HERE, "argv.json"), "w") as f:
    json.dump(args, f)
params = dict(kv.split("=", 1) for kv in args[args.index("--parameters") + 1].split(","))
if MODE == "ok":
    print("\nStarting session with SessionId: qurry-test-0123456789abcdef", flush=True)
    p = subprocess.Popen([sys.executable, os.path.abspath(__file__), "--plugin", params["host"], params["portNumber"], params["localPortNumber"]])
    with open(os.path.join(HERE, "pids"), "w") as f:
        f.write(f"{os.getpid()} {p.pid}")
    sys.exit(p.wait())
if MODE == "hang":
    with open(os.path.join(HERE, "pids"), "w") as f:
        f.write(f"{os.getpid()}")
    time.sleep(3600)
if MODE == "plugin":
    sys.stderr.write("\nSessionManagerPlugin is not found. Please refer to SessionManager Documentation here: http://docs.aws.amazon.com/console/systems-manager/session-manager-plugin-not-found\n")
elif MODE == "sso":
    sys.stderr.write("\nError when retrieving token from sso: Token has expired and refresh failed\n")
elif MODE == "early":
    sys.stderr.write("\nAn error occurred (TargetNotConnected) when calling the StartSession operation: i-0123456789abcdef0 is not connected.\n")
sys.exit(255)
"#;

    pub(crate) fn fake_aws(dir: &Path, mode: &str) -> std::path::PathBuf {
        let p = dir.join("aws");
        std::fs::write(&p, FAKE.replace("@MODE@", mode)).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn cfg() -> SsmConfig {
        SsmConfig { enabled: true, profile: "dev".into(), region: "ap-northeast-2".into(), target: "i-0123456789abcdef0".into() }
    }

    /// Alive and not a zombie (a killed child stays a zombie until it is reaped)
    fn alive(pid: &str) -> bool {
        let o = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid]).output().unwrap();
        let s = String::from_utf8_lossy(&o.stdout);
        !s.trim().is_empty() && !s.trim().starts_with('Z')
    }

    async fn gone(dir: &Path) {
        let pids = std::fs::read_to_string(dir.join("pids")).unwrap();
        for _ in 0..50 {
            if !pids.split_whitespace().any(alive) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("still running after drop: {pids}");
    }

    async fn open_fake(mode: &str, host: &str, port: u16, timeout: Duration) -> (tempfile::TempDir, AppResult<Tunnel>) {
        let dir = tempfile::tempdir().unwrap();
        let aws = fake_aws(dir.path(), mode);
        let t = open_with(&aws, None, &cfg(), host, port, timeout).await;
        (dir, t)
    }

    #[tokio::test]
    async fn forwards_through_the_cli_and_stops_it_on_drop() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        let (dir, t) = open_fake("ok", "127.0.0.1", echo_port, Duration::from_secs(10)).await;
        let t = t.expect("tunnel");
        let argv: Vec<String> = serde_json::from_str(&std::fs::read_to_string(dir.path().join("argv.json")).unwrap()).unwrap();
        let params = format!("host=127.0.0.1,portNumber={echo_port},localPortNumber={}", t.local_port);
        assert_eq!(
            argv,
            ["ssm", "start-session", "--target", "i-0123456789abcdef0", "--document-name", "AWS-StartPortForwardingSessionToRemoteHost", "--parameters", &params, "--profile", "dev", "--region", "ap-northeast-2"]
        );
        // Several connections at once, like a pool
        for _ in 0..3 {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", t.local_port)).await.unwrap();
            s.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            s.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
        }
        let port = t.local_port;
        drop(t);
        gone(dir.path()).await;
        assert!(tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err(), "the port is closed with the plugin");
    }

    #[tokio::test]
    async fn cli_missing_says_how_to_install() {
        let dir = tempfile::tempdir().unwrap();
        let e = open_with(&dir.path().join("aws"), None, &cfg(), "db", 5432, Duration::from_secs(5)).await.err().unwrap().to_string();
        assert!(e.contains("AWS CLI not found") && e.contains(INSTALL_CLI), "{e}");
    }

    #[tokio::test]
    async fn cli_errors_say_what_to_do() {
        for (mode, want) in [
            ("plugin", "brew install --cask session-manager-plugin"),
            ("sso", "Run: aws sso login --profile dev"),
            ("early", "i-0123456789abcdef0 is not connected to Session Manager"),
        ] {
            let (_dir, t) = open_fake(mode, "db.internal", 5432, Duration::from_secs(10)).await;
            let e = t.err().expect(mode).to_string();
            assert!(e.contains(want), "{mode}: {e}");
        }
    }

    #[tokio::test]
    async fn times_out_and_stops_the_cli() {
        let start = Instant::now();
        let (dir, t) = open_fake("hang", "db.internal", 5432, Duration::from_secs(1)).await;
        let e = t.err().unwrap().to_string();
        assert!(e.contains("didn't open local port") && e.contains("within 1s"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(5));
        gone(dir.path()).await;
    }

    #[test]
    fn values_that_could_pass_options_are_refused() {
        let ok = args(&cfg(), "mydb.cluster-abc.ap-northeast-2.rds.amazonaws.com", 5432, 40000).unwrap();
        assert_eq!(ok[7], "host=mydb.cluster-abc.ap-northeast-2.rds.amazonaws.com,portNumber=5432,localPortNumber=40000");
        let no_profile = args(&SsmConfig { profile: String::new(), region: String::new(), ..cfg() }, "10.0.1.5", 3306, 1).unwrap();
        assert_eq!(no_profile.len(), 8, "empty profile / region leave the CLI's defaults: {no_profile:?}");
        assert!(args(&SsmConfig { target: "mi-0123456789abcdef0".into(), ..cfg() }, "db", 1, 1).is_ok());
        for target in ["", "i-xyz", "i-0123", "i-0123456789abcdef0 --debug", "--debug", "I-0123456789ABCDEF0"] {
            assert!(args(&SsmConfig { target: target.into(), ..cfg() }, "db", 1, 1).is_err(), "{target}");
        }
        for profile in ["--debug", "a b", "a;rm", "x\ny"] {
            assert!(args(&SsmConfig { profile: profile.into(), ..cfg() }, "db", 1, 1).is_err(), "{profile}");
        }
        for region in ["US-EAST-1", "--endpoint-url", "us east"] {
            assert!(args(&SsmConfig { region: region.into(), ..cfg() }, "db", 1, 1).is_err(), "{region}");
        }
        for host in ["", "db,portNumber=22", "db=x", "-db", "a b", "::1"] {
            assert!(args(&cfg(), host, 1, 1).is_err(), "{host}");
        }
    }

    /// The real CLI's messages (AWS CLI v2) → what to do
    #[test]
    fn explains_the_cli_messages() {
        let c = cfg();
        let cases = [
            ("\nThe SSO session associated with this profile has expired or is otherwise invalid. To refresh this SSO session run aws sso login with the corresponding profile.\n", "Run: aws sso login --profile dev"),
            ("Error loading SSO Token: Token for my-sso does not exist", "Run: aws sso login --profile dev"),
            ("An error occurred (AccessDeniedException) when calling the StartSession operation: User: arn:aws:sts::123456789012:assumed-role/dev/me is not authorized to perform: ssm:StartSession on resource: arn:aws:ec2:ap-northeast-2:123456789012:instance/i-0123456789abcdef0", "Access denied"),
            ("An error occurred (ExpiredTokenException) when calling the StartSession operation: The security token included in the request is expired", "credentials have expired"),
            ("\nUnable to locate credentials. You can configure credentials by running \"aws configure\".\n", "found no credentials"),
            ("\nThe config profile (dev) could not be found\n", "isn't in your AWS config"),
            ("\nYou must specify a region. You can also configure your region by running \"aws configure\".\n", "No AWS region"),
            ("\nAn error occurred (InvalidInstanceId) when calling the StartSession operation: \n", "failed: An error occurred (InvalidInstanceId)"),
        ];
        for (out, want) in cases {
            let e = explain(out, &c);
            assert!(e.contains(want), "{out}\n→ {e}");
        }
        assert!(explain("", &SsmConfig { profile: String::new(), ..c.clone() }).contains("without saying why"));
        assert!(explain("Token has expired and refresh failed sso", &SsmConfig { profile: String::new(), ..c }).contains("Run: aws sso login —"));
    }

    #[tokio::test]
    async fn both_tunnels_on_is_refused() {
        let mut c: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "db.internal", "port": 5432, "user": "u",
            "ssh": { "enabled": true, "host": "bastion", "user": "me" } })).unwrap();
        c.ssm = Some(cfg());
        let e = crate::app::connect_config(&c, None, None).await.err().unwrap().to_string();
        assert!(e.contains("choose one"), "{e}");
    }

    /// Fake aws forwarding to a real PostgreSQL, through AppState as the apps use it: Test, connect, query, disconnect.
    /// Run: QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-core ssm_tunnel_end_to_end -- --ignored
    #[tokio::test]
    #[ignore]
    async fn ssm_tunnel_end_to_end() {
        let pg: Vec<String> = std::env::var("QURRY_TEST_PG").expect("QURRY_TEST_PG").split(':').map(String::from).collect();
        let fake = tempfile::tempdir().unwrap();
        fake_aws(fake.path(), "ok");
        // Only this test changes PATH (run it on its own): the fake comes first, like an aws in /opt/homebrew/bin would
        let path = std::env::join_paths(std::iter::once(fake.path().to_path_buf()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
        unsafe { std::env::set_var("PATH", path) };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::app::AppState::new(crate::store::Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut c: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "ssm", "kind": "postgres", "host": pg[0], "port": pg[1].parse::<u16>().unwrap(), "user": pg[2], "database": pg[3] })).unwrap();
        c.ssm = Some(cfg());
        let msg = st.test_connection(c.clone(), None, None).await.expect("test via ssm");
        assert!(msg.starts_with("PostgreSQL") && msg.contains("(via AWS SSM i-0123456789abcdef0)"), "{msg}");
        gone(fake.path()).await;
        let saved = st.save_connection(c, None, None).await.unwrap();
        st.connect(saved.id.clone()).await.expect("connect via ssm");
        let r = st.run_query(saved.id.clone(), "select count(*) from generate_series(1, 1000)".into(), Some(10), None).await.unwrap();
        assert_eq!(r.rows[0][0], serde_json::Value::from(1000));
        let argv = std::fs::read_to_string(fake.path().join("argv.json")).unwrap();
        assert!(argv.contains(&format!("host={},portNumber={}", pg[0], pg[1])), "{argv}");
        st.disconnect(saved.id).await.unwrap();
        gone(fake.path()).await;
    }
}
