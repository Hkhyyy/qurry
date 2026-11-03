//! AWS SSM Session Manager tunnel: runs `aws ssm start-session` with the AWS-StartPortForwardingSessionToRemoteHost document on a free
//! local port, waits until the plugin says it opened that port, and when the Tunnel is dropped, and when the app exits, panics or is
//! told to stop (kill_all), ends the session on the AWS side (`aws ssm terminate-session`: a stopped CLI or plugin leaves it open on
//! the instance, whose agent stops opening new ones after a dozen) and stops the CLI and the Session Manager plugin it started (they
//! would otherwise keep the port open).
//! Qurry stores no AWS secrets: the AWS CLI uses the user's own config, SSO login or environment. Arguments go as a vector (no shell)
//! and are checked first, so a data source can't add CLI options or document parameters.
use crate::error::{AppError, AppResult};
use crate::store::SsmConfig;
use crate::tunnel::Tunnel;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};

/// Starting a session (credentials, StartSession, the plugin's websocket) usually takes a few seconds
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALL_CLI: &str = "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html";
const INSTALL_PLUGIN: &str = "https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html";
/// How much of the CLI's latest output is kept for the error message
const TAIL: usize = 4096;

/// How long the CLI and the plugin get to end the session on the AWS side before they are killed
const GRACE: Duration = Duration::from_millis(1500);

/// How long `aws ssm terminate-session` may take when a Tunnel closes (in the background), and in all when the app exits (kill_all)
const TERMINATE: Duration = Duration::from_secs(5);
const TERMINATE_ON_EXIT: Duration = Duration::from_secs(2);

/// The CLI's line with the session's id, before the plugin starts
const STARTING: &str = "Starting session with SessionId:";
/// The plugin's line when the instance couldn't connect to host:portNumber (it then closes the local connection)
const UNREACHABLE: &str = "Connection to destination port failed";

/// Process groups of the running CLIs (each is its group's leader) and how to end their sessions, for kill_all
static GROUPS: Mutex<Vec<(u32, Ender)>> = Mutex::new(Vec::new());

/// What the CLI and the plugin said that Qurry acts on
#[derive(Default)]
pub(crate) struct Heard {
    /// The session's id, for terminate-session
    id: OnceLock<String>,
    /// How many connections the instance couldn't forward to the database
    unreachable: AtomicUsize,
}

impl Heard {
    fn line(&self, line: &str) {
        if let Some(id) = line.trim().strip_prefix(STARTING).map(str::trim) {
            // Passed as an argument: an id that could pass an option is not used
            if !id.is_empty() && id.len() <= 128 && !id.starts_with('-') && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.@+=,:".contains(c)) {
                let _ = self.id.set(id.to_string());
            }
        }
        if line.contains(UNREACHABLE) {
            self.unreachable.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Taken before connecting through an SSM tunnel, to tell afterwards whether the instance couldn't reach the database meanwhile
pub(crate) struct Mark(Arc<Heard>, usize);

/// A connect through the tunnel failed. When the plugin said the instance couldn't reach host:port, say that: the driver only saw
/// the connection closed (PostgreSQL: "unexpected response from SSLRequest: 0x00")
pub(crate) async fn connect_error(mark: Option<Mark>, host: &str, port: u16, e: AppError) -> AppError {
    let Some(Mark(heard, before)) = mark else { return e };
    // The server answered: the instance reached it
    if matches!(e, AppError::Db(sqlx::Error::Database(_))) {
        return e;
    }
    // The plugin says it about when it closes the connection: give its line a moment to arrive
    for _ in 0..20 {
        if heard.unreachable.load(Ordering::SeqCst) > before {
            return AppError::Msg(qurry_shared::errors::ssm_unreachable(host, port));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    e
}

/// Ends a session on the AWS side with the CLI that started it (same PATH, profile and region)
#[derive(Clone)]
struct Ender {
    aws: PathBuf,
    path: Option<OsString>,
    /// --profile / --region, as checked by args()
    account: Vec<String>,
    heard: Arc<Heard>,
}

impl Ender {
    /// Starts `aws ssm terminate-session`, once the CLI has said the session's id
    fn start(&self) -> Option<std::process::Child> {
        let id = self.heard.id.get()?;
        let mut cmd = std::process::Command::new(&self.aws);
        cmd.args(["ssm", "terminate-session", "--session-id", id]).args(&self.account).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(p) = &self.path {
            cmd.env("PATH", p);
        }
        cmd.spawn().ok()
    }
}

/// Waits for the commands until `end`, then kills those still running
fn finish(cmds: Vec<std::process::Child>, end: Instant) {
    for mut c in cmds {
        while matches!(c.try_wait(), Ok(None)) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        if matches!(c.try_wait(), Ok(None)) {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// The running `aws ssm start-session`. Dropping it ends the session and stops the CLI and everything it started
pub struct Process {
    /// The CLI's pid (its process group) and the CLI itself, until the group is stopped
    child: Mutex<Option<(u32, Child)>>,
    ender: Ender,
}

impl Process {
    fn new(child: Child, ender: Ender) -> Self {
        let child = child.id().map(|pid| {
            GROUPS.lock().unwrap().push((pid, ender.clone()));
            (pid, child)
        });
        Self { child: Mutex::new(child), ender }
    }

    pub(crate) fn mark(&self) -> Mark {
        Mark(self.ender.heard.clone(), self.ender.heard.unreachable.load(Ordering::SeqCst))
    }

    /// The CLI is still running (it exits when the session ends: idle timeout, network, session terminated).
    /// Once it has exited, what is left of its group (a plugin that outlived it holds the port) is stopped right away
    pub(crate) fn alive(&self) -> bool {
        let mut g = self.child.lock().unwrap();
        let Some((pid, c)) = g.as_mut() else { return false };
        if matches!(c.try_wait(), Ok(None)) {
            return true;
        }
        let pid = *pid;
        g.take();
        close(pid, None, &self.ender);
        false
    }

    fn exited(&self) -> bool {
        !self.alive()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some((pid, child)) = self.child.lock().unwrap().take() {
            close(pid, Some(child), &self.ender);
        }
    }
}

/// The one way a session ends: terminate-session on the AWS side (in the background, up to TERMINATE) and, at the same time, the
/// group stopped as below (the port is free at once; the AWS call doesn't need the CLI or the plugin)
fn close(pid: u32, child: Option<Child>, ender: &Ender) {
    let ending = ender.start();
    stop_group(pid, child);
    if let Some(c) = ending {
        std::thread::spawn(move || finish(vec![c], Instant::now() + TERMINATE));
    }
}

/// Forgets the group and stops what is left of it. The CLI runs session-manager-plugin as its own child, which holds the port and
/// the session, so the whole group is stopped: SIGTERM first so they can end the session, SIGKILL after GRACE (without holding up
/// the caller). A group is only signalled while it has members: a pgid can't be reused until its last member is gone, but after
/// that the number may belong to another program's group. `child` is the CLI when it hasn't been reaped yet
fn stop_group(pid: u32, child: Option<Child>) {
    GROUPS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(p, _)| *p != pid);
    #[cfg(unix)]
    {
        let mut child = child;
        let mut reap = move || {
            if let Some(c) = child.as_mut() {
                let _ = c.try_wait();
            }
        };
        reap();
        if group_alive(pid) {
            signal_group(pid, libc::SIGTERM);
            std::thread::spawn(move || {
                if !wait_group(pid, GRACE, reap) {
                    signal_group(pid, libc::SIGKILL);
                }
            });
        }
    }
    // Elsewhere kill_on_drop stops the CLI only
    #[cfg(not(unix))]
    drop((pid, child));
}

#[cfg(unix)]
fn group_alive(pid: u32) -> bool {
    unsafe { libc::killpg(pid as libc::pid_t, 0) == 0 }
}

#[cfg(unix)]
fn signal_group(pid: u32, sig: libc::c_int) {
    unsafe { libc::killpg(pid as libc::pid_t, sig) };
}

/// Waits until nothing is left in the group (true) or `within` has passed
#[cfg(unix)]
fn wait_group(pid: u32, within: Duration, mut reap: impl FnMut()) -> bool {
    let end = Instant::now() + within;
    loop {
        reap();
        if !group_alive(pid) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ends every SSM session and stops its CLI and plugin now. For exit paths where the Tunnels are never dropped: the GUI quitting
/// (the process exits without dropping its state), a panic (abort), a signal, or the TUI relaunching itself after an update. The
/// sessions are terminated at the same time, within TERMINATE_ON_EXIT in all, so quitting stays quick
pub fn kill_all() {
    end_now(std::mem::take(&mut *GROUPS.lock().unwrap_or_else(|e| e.into_inner())));
}

fn end_now(groups: Vec<(u32, Ender)>) {
    let end = Instant::now() + TERMINATE_ON_EXIT;
    let ending = groups.iter().filter_map(|(_, e)| e.start()).collect();
    stop_now(&groups.iter().map(|(p, _)| *p).collect::<Vec<_>>());
    finish(ending, end);
}

/// SIGTERM, a short grace (the app is exiting), then SIGKILL
fn stop_now(groups: &[u32]) {
    #[cfg(unix)]
    {
        // GROUPS only holds CLIs that haven't been reaped, so these pids are still theirs
        for pid in groups {
            signal_group(*pid, libc::SIGTERM);
        }
        for pid in groups {
            if !wait_group(*pid, Duration::from_millis(300), || {}) {
                signal_group(*pid, libc::SIGKILL);
            }
        }
    }
}

/// kill_all on a panic (the release builds abort, so no Drop runs), before the hook already set
pub fn kill_on_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        kill_all();
        hook(p);
    }));
}

/// Waits for SIGTERM / SIGINT / SIGHUP (an MCP host stopping the server, the terminal closing), stops the SSM processes and exits
pub async fn kill_on_signals() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt()), signal(SignalKind::hangup())) else { return };
        let code = tokio::select! {
            _ = term.recv() => libc::SIGTERM,
            _ = int.recv() => libc::SIGINT,
            _ = hup.recv() => libc::SIGHUP,
        };
        kill_all();
        std::process::exit(128 + code);
    }
}

fn cli_missing() -> AppError {
    AppError::Msg(format!("AWS CLI not found. Install AWS CLI v2 ({INSTALL_CLI}, or brew install awscli), then try again."))
}

/// Opens the tunnel to `host:port` as seen from the target instance (the data source's host/port)
pub async fn open(ssm: &SsmConfig, host: &str, port: u16) -> AppResult<Tunnel> {
    // A GUI app started from the Dock has a short PATH: look where the AWS installers put the CLI and the plugin too
    open_in(&crate::backup::tool_dirs(), ssm, host, port, READY_TIMEOUT).await
}

/// The first `aws` in `dirs`, run with `dirs` as its PATH (the CLI runs session-manager-plugin from PATH)
async fn open_in(dirs: &[PathBuf], ssm: &SsmConfig, host: &str, port: u16, timeout: Duration) -> AppResult<Tunnel> {
    let aws = dirs.iter().map(|d| d.join("aws")).find(|p| p.is_file()).ok_or_else(cli_missing)?;
    open_with(&aws, std::env::join_paths(dirs).ok(), ssm, host, port, timeout).await
}

async fn open_with(aws: &Path, path: Option<OsString>, ssm: &SsmConfig, host: &str, port: u16, timeout: Duration) -> AppResult<Tunnel> {
    let local = free_port()?;
    let mut cmd = Command::new(aws);
    let ender = Ender { aws: aws.to_path_buf(), path: path.clone(), account: account(ssm), heard: Arc::default() };
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
    let heard = ender.heard.clone();
    let readers = [drain(child.stdout.take(), tail.clone(), heard.clone()), drain(child.stderr.take(), tail.clone(), heard)];
    let child = Process::new(child, ender);
    // The plugin prints this once it listens on the port. Until then the port is free (free_port let it go), and another
    // program on this machine could have taken it: a successful connect alone doesn't say the plugin is behind it
    let opened = format!("Port {local} opened for sessionId");
    let deadline = Instant::now() + timeout;
    loop {
        if child.exited() {
            for r in readers {
                let _ = tokio::time::timeout(Duration::from_secs(1), r).await;
            }
            let out = tail.lock().unwrap().clone();
            return Err(AppError::Msg(explain(&out, ssm)));
        }
        if tail.lock().unwrap().contains(&opened) && tokio::net::TcpStream::connect(("127.0.0.1", local)).await.is_ok() {
            return Ok(Tunnel::process(local, child));
        }
        if Instant::now() >= deadline {
            let out = tail.lock().unwrap().clone();
            let last = last_line(&out);
            // The CLI started the session, but the plugin's data channel to the agent never opened
            if last.as_deref().is_some_and(|l| l.starts_with(STARTING)) {
                return Err(AppError::Msg(format!(
                    "The AWS SSM session to {} started, but its data channel didn't open within {}s. Check the instance's SSM agent: it may have too many open sessions (they time out on their own).",
                    ssm.target.trim(),
                    timeout.as_secs()
                )));
            }
            let last = last.map(|l| format!(" Last output: {l}")).unwrap_or_default();
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

fn drain(r: Option<impl AsyncRead + Unpin + Send + 'static>, tail: Arc<Mutex<String>>, heard: Arc<Heard>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(r) = r else { return };
        let mut r = BufReader::new(r);
        let mut line = Vec::new();
        while r.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
            let text = String::from_utf8_lossy(&line);
            heard.line(&text);
            let mut t = tail.lock().unwrap();
            t.push_str(&text);
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
    if target.is_empty() {
        return Err(AppError::Msg("Enter the instance id (i-…) for the AWS SSM tunnel (GUI: SSH/SSL tab · TUI: SSM target).".into()));
    }
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
    a.extend(account(ssm));
    Ok(a)
}

/// --profile / --region when set (args() checks them)
fn account(ssm: &SsmConfig) -> Vec<String> {
    let mut a = Vec::new();
    for (flag, v) in [("--profile", ssm.profile.trim()), ("--region", ssm.region.trim())] {
        if !v.is_empty() {
            a.extend([flag.to_string(), v.to_string()]);
        }
    }
    a
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
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A fake `aws` that prints what the real CLI prints. "ok" starts a fake plugin (a child, like the real CLI) that listens on
    /// localPortNumber and forwards to host:portNumber; it writes its argv and the pids next to itself, and a line per start to
    /// `starts`. "silent" is "ok" with a plugin that never says it opened the port; "stuck" starts the session but no plugin (the
    /// data channel never opens); "onpath" is "ok" when session-manager-plugin is on its PATH and "plugin" (the real CLI's message)
    /// when it isn't. `ssm terminate-session` writes its argv as a line of `terminated`, and then hangs if `terminate-hangs` exists
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
    if MODE != "silent":
        print(f"Port {local} opened for sessionId qurry-test-0123456789abcdef.", flush=True)
    print("Waiting for connections...", flush=True)
    while True:
        c, _ = srv.accept()
        print("\nConnection accepted for session [qurry-test-0123456789abcdef]", flush=True)
        try:
            u = socket.create_connection((host, port))
        except OSError:
            print("Connection to destination port failed, check SSM Agent logs.", flush=True)
            c.close()
            continue
        threading.Thread(target=pipe, args=(c, u), daemon=True).start()
        threading.Thread(target=pipe, args=(u, c), daemon=True).start()
args = sys.argv[1:]
if args[:2] == ["ssm", "terminate-session"]:
    with open(os.path.join(HERE, "terminated"), "a") as f:
        f.write(json.dumps(args) + "\n")
    if os.path.exists(os.path.join(HERE, "terminate-hangs")):
        time.sleep(3600)
    print(json.dumps({"SessionId": args[3]}))
    sys.exit(0)
with open(os.path.join(HERE, "pids"), "w") as f:
    f.write(f"{os.getpid()}")
if MODE == "onpath":
    import shutil
    if not shutil.which("session-manager-plugin"):
        MODE = "plugin"
    else:
        MODE = "ok"
with open(os.path.join(HERE, "argv.json"), "w") as f:
    json.dump(args, f)
params = dict(kv.split("=", 1) for kv in args[args.index("--parameters") + 1].split(","))
if MODE in ("ok", "silent"):
    with open(os.path.join(HERE, "starts"), "a") as f:
        f.write(f"{os.getpid()}\n")
    print("\nStarting session with SessionId: qurry-test-0123456789abcdef", flush=True)
    p = subprocess.Popen([sys.executable, os.path.abspath(__file__), "--plugin", params["host"], params["portNumber"], params["localPortNumber"]])
    with open(os.path.join(HERE, "pids"), "w") as f:
        f.write(f"{os.getpid()} {p.pid}")
    sys.exit(p.wait())
if MODE == "stuck":
    print("\nStarting session with SessionId: qurry-test-0123456789abcdef", flush=True)
if MODE in ("hang", "stuck"):
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
        // An absolute interpreter: some tests give the CLI a PATH without python3
        let python = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).map(|d| d.join("python3")).find(|p| p.is_file()).expect("python3 on PATH");
        let script = FAKE.replace("@MODE@", mode).replacen("/usr/bin/env python3", &python.to_string_lossy(), 1);
        std::fs::write(&p, script).unwrap();
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

    pub(crate) async fn gone(dir: &Path) {
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

    /// An open tunnel through the fake aws (to 127.0.0.1:9), for the app tests. Keep the folder until it is gone
    pub(crate) async fn fake_tunnel() -> (tempfile::TempDir, Tunnel) {
        let (dir, t) = open_fake("ok", "127.0.0.1", 9, Duration::from_secs(10)).await;
        (dir, t.expect("fake tunnel"))
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
        assert_eq!(terminated(dir.path()).await, [TERMINATED], "the session is ended on the AWS side");
    }

    /// terminate-session with the session id the CLI printed, and the data source's profile and region
    const TERMINATED: [&str; 8] = ["ssm", "terminate-session", "--session-id", "qurry-test-0123456789abcdef", "--profile", "dev", "--region", "ap-northeast-2"];

    /// The terminate-session calls of the fake, once at least one has been written (or after 5s)
    async fn terminated(dir: &Path) -> Vec<Vec<String>> {
        for _ in 0..50 {
            if let Some(s) = std::fs::read_to_string(dir.join("terminated")).ok().filter(|s| s.ends_with('\n')) {
                return s.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Vec::new()
    }

    /// Disconnecting the data source (drop_pool) ends the session on the AWS side
    #[tokio::test]
    async fn disconnect_terminates_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let (fake, t) = fake_tunnel().await;
        st.tunnels.write().await.insert("src".into(), t);
        st.disconnect("src".into()).await.unwrap();
        gone(fake.path()).await;
        assert_eq!(terminated(fake.path()).await, [TERMINATED]);
    }

    /// Quitting doesn't wait long for AWS: kill_all ends the sessions together, within TERMINATE_ON_EXIT, and stops the groups
    #[tokio::test]
    async fn kill_all_is_quick_when_terminate_hangs() {
        let (dir, t) = open_fake("ok", "127.0.0.1", 9, Duration::from_secs(10)).await;
        let t = t.expect("tunnel");
        std::fs::write(dir.path().join("terminate-hangs"), "").unwrap();
        let cli = cli_pid(dir.path());
        // Only this test's group: kill_all would stop the other tests' tunnels too
        let mine: Vec<_> = {
            let mut g = GROUPS.lock().unwrap();
            let (mine, rest) = std::mem::take(&mut *g).into_iter().partition(|(p, _)| *p == cli);
            *g = rest;
            mine
        };
        assert_eq!(mine.len(), 1);
        let start = Instant::now();
        end_now(mine);
        let took = start.elapsed();
        assert!(took >= TERMINATE_ON_EXIT - Duration::from_millis(100) && took < TERMINATE_ON_EXIT + Duration::from_secs(1), "{took:?}");
        gone(dir.path()).await;
        assert_eq!(terminated(dir.path()).await, [TERMINATED]);
        std::mem::forget(t);
    }

    /// The instance couldn't reach host:port (General tab pointing at a local port): the plugin says so and closes the connection,
    /// and the connect error says what Host and Port mean with SSM instead of the driver's "unexpected response from SSLRequest: 0x00"
    #[tokio::test]
    async fn an_unreachable_database_says_what_host_and_port_are() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (_dir, t) = open_fake("ok", "127.0.0.1", closed, Duration::from_secs(10)).await;
        let t = t.expect("tunnel");
        let c: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "127.0.0.1", "port": closed, "user": "u" })).unwrap();
        let mark = t.ssm_mark();
        let e = crate::db::connect_via(&c, None, Some(t.local_port)).await.err().expect("nothing behind it");
        let raw = e.to_string();
        let e = connect_error(mark, &c.host, c.port, e).await.to_string();
        assert_eq!(e, qurry_shared::errors::ssm_unreachable("127.0.0.1", closed), "driver said: {raw}");
        // Not said for a failure after an earlier one (the count moved before the mark), nor without SSM
        let mark = t.ssm_mark();
        let e = connect_error(mark, &c.host, c.port, AppError::Msg("password authentication failed".into())).await.to_string();
        assert_eq!(e, "password authentication failed");
        assert_eq!(connect_error(None, "h", 1, AppError::Msg("x".into())).await.to_string(), "x");
    }

    /// The session started but the plugin never opened the port (the instance's agent has too many sessions): the error says so,
    /// and the session is ended on the AWS side
    #[tokio::test]
    async fn a_session_stuck_at_starting_says_so_and_is_terminated() {
        let (dir, t) = open_fake("stuck", "db.internal", 5432, Duration::from_secs(2)).await;
        let e = t.err().unwrap().to_string();
        assert_eq!(
            e,
            "The AWS SSM session to i-0123456789abcdef0 started, but its data channel didn't open within 2s. Check the instance's SSM agent: it may have too many open sessions (they time out on their own)."
        );
        gone(dir.path()).await;
        assert_eq!(terminated(dir.path()).await, [TERMINATED]);
    }

    /// Something listening on the port is not enough: the plugin has to say it opened it (another local program could hold it)
    #[tokio::test]
    async fn waits_for_the_plugin_to_open_the_port() {
        let (dir, t) = open_fake("silent", "127.0.0.1", 9, Duration::from_secs(3)).await;
        let e = t.err().expect("not ready without the plugin's line").to_string();
        assert!(e.contains("didn't open local port") && e.contains("Last output: Waiting for connections"), "{e}");
        gone(dir.path()).await;
        assert_eq!(terminated(dir.path()).await, [TERMINATED], "a failed start ends its session too");
    }

    /// A tunnel whose CLI has exited says so (ensure_pool then opens it again), and stop_now (kill_all) stops a CLI and its plugin
    #[tokio::test]
    async fn notices_a_stopped_cli_and_stops_groups_now() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = echo.local_addr().unwrap().port();
        let (dir, t) = open_fake("ok", "127.0.0.1", port, Duration::from_secs(10)).await;
        let t = t.expect("tunnel");
        assert!(t.alive());
        let pids = std::fs::read_to_string(dir.path().join("pids")).unwrap();
        let cli: u32 = pids.split_whitespace().next().unwrap().parse().unwrap();
        stop_now(&[cli]);
        gone(dir.path()).await;
        for _ in 0..50 {
            if !t.alive() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!t.alive(), "the exited CLI is noticed");
    }

    fn cli_pid(dir: &Path) -> u32 {
        std::fs::read_to_string(dir.join("pids")).unwrap().split_whitespace().next().unwrap().parse().unwrap()
    }

    fn in_groups(pid: u32) -> bool {
        GROUPS.lock().unwrap().iter().any(|(p, _)| *p == pid)
    }

    /// A CLI that exited (here: a failed start) is forgotten, so kill_all never signals its pid, which may be reused by then
    #[tokio::test]
    async fn a_failed_start_leaves_no_group_behind() {
        for mode in ["early", "sso", "plugin"] {
            let (dir, t) = open_fake(mode, "db.internal", 5432, Duration::from_secs(10)).await;
            assert!(t.is_err(), "{mode}");
            assert!(!in_groups(cli_pid(dir.path())), "{mode}: the exited CLI's pid is still kept for kill_all");
            assert!(!dir.path().join("terminated").exists(), "{mode}: no session, nothing to terminate");
        }
    }

    /// The CLI dies but its plugin lives on (a crash, an OOM kill): the plugin still holds the forward to the database. Noticing the
    /// exit stops the rest of the group and forgets the pid; dropping the Tunnel afterwards signals nothing
    #[tokio::test]
    async fn a_plugin_that_outlives_its_cli_is_stopped() {
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        let (dir, t) = open_fake("ok", "127.0.0.1", echo_port, Duration::from_secs(10)).await;
        let t = t.expect("tunnel");
        let cli = cli_pid(dir.path());
        assert!(in_groups(cli));
        unsafe { libc::kill(cli as libc::pid_t, libc::SIGKILL) };
        for _ in 0..50 {
            if !t.alive() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!t.alive());
        assert!(!in_groups(cli), "the exited CLI's pid is forgotten");
        gone(dir.path()).await;
        let port = t.local_port;
        drop(t);
        assert!(tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err(), "nothing forwards on the port any more");
    }

    /// `open` finds aws in the tool folders and gives the CLI those folders as PATH, where it finds session-manager-plugin
    #[tokio::test]
    async fn finds_the_cli_and_the_plugin_in_the_tool_folders() {
        let dir = tempfile::tempdir().unwrap();
        fake_aws(dir.path(), "onpath");
        let e = open_in(&[dir.path().to_path_buf()], &cfg(), "127.0.0.1", 9, Duration::from_secs(10)).await.err().expect("no plugin yet").to_string();
        assert!(e.contains("brew install --cask session-manager-plugin"), "{e}");
        let plugin = dir.path().join("session-manager-plugin");
        std::fs::write(&plugin, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let t = open_in(&[dir.path().join("empty"), dir.path().to_path_buf()], &cfg(), "127.0.0.1", 9, Duration::from_secs(10)).await.expect("tunnel");
        drop(t);
        gone(dir.path()).await;
        let empty = tempfile::tempdir().unwrap();
        let e = open_in(&[empty.path().to_path_buf()], &cfg(), "db", 5432, Duration::from_secs(5)).await.err().unwrap().to_string();
        assert!(e.contains("AWS CLI not found") && e.contains(INSTALL_CLI), "{e}");
    }

    #[tokio::test]
    async fn empty_target_is_an_error_not_a_direct_connection() {
        let mut c: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "localhost", "port": 5432, "user": "u" })).unwrap();
        c.ssm = Some(SsmConfig { target: " ".into(), ..cfg() });
        assert!(c.tunneled());
        let e = crate::app::connect_config(&c, None, None).await.err().unwrap().to_string();
        assert!(e.contains("Enter the instance id"), "{e}");
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
        let (dir, t) = open_fake("hang", "db.internal", 5432, Duration::from_secs(3)).await;
        let e = t.err().unwrap().to_string();
        assert!(e.contains("didn't open local port") && e.contains("within 3s"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(8));
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
            // aws-cli 2.x on this machine, without credentials / config
            ("aws: [ERROR]: Error loading SSO Token: Token for my-sso does not exist\n", "Run: aws sso login --profile dev"),
            ("aws: [ERROR]: An error occurred (NoCredentials): Unable to locate credentials. You can configure credentials by running \"aws login\".\n", "found no credentials"),
            ("aws: [ERROR]: The config profile (nope) could not be found\n", "isn't in your AWS config"),
            ("aws: [ERROR]: An error occurred (NoRegion): You must specify a region. You can also configure your region by running \"aws configure\".\n", "No AWS region"),
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
        let pg = crate::testenv::pg().expect("QURRY_TEST_PG").cfg;
        let fake = tempfile::tempdir().unwrap();
        fake_aws(fake.path(), "ok");
        // Only this test changes PATH (run it on its own): the fake comes first, like an aws in /opt/homebrew/bin would
        let path = std::env::join_paths(std::iter::once(fake.path().to_path_buf()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
        unsafe { std::env::set_var("PATH", path) };
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let mut c: crate::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "ssm", "kind": "postgres", "host": &pg.host, "port": pg.port, "user": &pg.user, "database": &pg.database })).unwrap();
        c.ssm = Some(cfg());
        let msg = st.test_connection(c.clone(), None, None).await.expect("test via ssm");
        assert!(msg.starts_with("PostgreSQL") && msg.contains("(via AWS SSM i-0123456789abcdef0)"), "{msg}");
        gone(fake.path()).await;
        let saved = st.save_connection(c, None, None).await.unwrap();
        st.connect(saved.id.clone()).await.expect("connect via ssm");
        let r = st.run_query(saved.id.clone(), "select count(*) from generate_series(1, 1000)".into(), Some(10), None).await.unwrap();
        assert_eq!(r.rows[0][0], serde_json::Value::from(1000));
        let argv = std::fs::read_to_string(fake.path().join("argv.json")).unwrap();
        assert!(argv.contains(&format!("host={},portNumber={}", pg.host, pg.port)), "{argv}");
        // One SSM session for the source, however many databases are opened, also when they are opened at the same time
        for db in ["qurry_ssm_a", "qurry_ssm_b"] {
            let _ = st.run_query(saved.id.clone(), format!("create database {db}"), None, None).await;
        }
        let starts = || std::fs::read_to_string(fake.path().join("starts")).unwrap().lines().count();
        let before = starts();
        st.warm_search_cache(saved.id.clone()).await.unwrap();
        assert!(st.pools.read().await.len() >= 3, "the default DB and the two new ones");
        assert_eq!(st.tunnels.read().await.len(), 1, "one tunnel for the source");
        assert_eq!(starts(), before, "no other aws CLI was started");
        // The CLI stops (idle timeout): the next uses open the session again (once, though they come together) instead of
        // connecting to a dead port
        let cli: u32 = std::fs::read_to_string(fake.path().join("pids")).unwrap().split_whitespace().next().unwrap().parse().unwrap();
        stop_now(&[cli]);
        gone(fake.path()).await;
        let targets = [saved.id.clone(), format!("{}::qurry_ssm_a", saved.id), format!("{}::qurry_ssm_b", saved.id), format!("{}::qurry_ssm_a", saved.id)];
        let pools = futures::future::join_all(targets.iter().map(|t| crate::app::ensure_pool(&st, t))).await;
        for p in pools {
            assert!(p.expect("pool").server_version().await.is_ok());
        }
        assert_eq!(starts(), before + 1, "one new session");
        assert_eq!(st.tunnels.read().await.len(), 1);
        // A missing database opened together with the default one fails alone, and the session stays for the next try
        crate::app::drop_pool(&st, &saved.id).await;
        gone(fake.path()).await;
        let missing = format!("{}::qurry_ssm_no_such_db", saved.id);
        let (bad, good) = tokio::join!(crate::app::ensure_pool(&st, &missing), crate::app::ensure_pool(&st, &saved.id));
        let bad = bad.err().expect("missing db").to_string();
        assert!(bad.contains("does not exist"), "{bad}");
        assert!(good.expect("the default db connects").server_version().await.is_ok());
        assert!(crate::app::ensure_pool(&st, &missing).await.is_err());
        assert_eq!(starts(), before + 2, "one session for both, kept after the failed database");
        crate::app::drop_pool(&st, &saved.id).await;
        for db in ["qurry_ssm_a", "qurry_ssm_b"] {
            let _ = st.run_query(saved.id.clone(), format!("drop database if exists {db} with (force)"), None, None).await;
        }
        st.disconnect(saved.id).await.unwrap();
        gone(fake.path()).await;
    }
}
