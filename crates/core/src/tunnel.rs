//! SSH tunnel: opens a local port and forwards incoming connections to the remote DB over SSH direct-tcpip channels.
//! The AWS SSM tunnel (ssm.rs) gives the same Tunnel: a local port that lives until it is dropped.
use crate::error::{AppError, AppResult};
use crate::store::{SshAuth, SshConfig};
use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Unknown server keys: (host, port) → key. Added to known_hosts when the user checks the fingerprint and calls trust_host_key.
/// None = a different key arrived after the fingerprint was shown (could be a MITM, so it isn't added)
static PENDING_KEYS: Mutex<Option<HashMap<(String, u16), Option<russh::keys::PublicKey>>>> = Mutex::new(None);

/// Remembers the key awaiting confirmation. If a different key is already waiting, trust neither
fn remember_pending(host: &str, port: u16, key: &russh::keys::PublicKey) {
    let mut g = PENDING_KEYS.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    let k = (host.to_string(), port);
    let next = match m.get(&k) {
        Some(Some(old)) if old != key => None,
        Some(None) => None,
        _ => Some(key.clone()),
    };
    m.insert(k, next);
}

/// Text included in the error message when connecting stops for an unknown SSH server (the UI shows "Trust host key" based on it)
pub const UNKNOWN_HOST: &str = "Unknown SSH host key";

fn known_hosts_path() -> Option<String> {
    // Tests use a separate file via QURRY_KNOWN_HOSTS (protects the user's ~/.ssh/known_hosts)
    std::env::var("QURRY_KNOWN_HOSTS").ok()
}

/// Adds a server key whose fingerprint the user confirmed to known_hosts (passes from the next connection on). `host` / `port` are
/// the data source's SSH host: when the unknown key came from its ~/.ssh/config HostName or a ProxyJump host, that one is trusted
pub fn trust_host_key(host: &str, port: u16) -> AppResult<String> {
    let pending = |h: &str, p: u16| PENDING_KEYS.lock().unwrap().as_ref().is_some_and(|m| m.contains_key(&(h.to_string(), p)));
    let (host, port) = match pending(host, port) {
        true => (host.to_string(), port),
        false => {
            let ssh = SshConfig { enabled: true, host: host.into(), port, user: String::new(), auth: SshAuth::Key, key_path: String::new() };
            route(&ssh).unwrap_or_default().into_iter().find(|h| pending(&h.host, h.port)).map(|h| (h.host, h.port)).unwrap_or((host.to_string(), port))
        }
    };
    let host = host.as_str();
    let key = PENDING_KEYS.lock().unwrap().as_mut().and_then(|m| m.remove(&(host.to_string(), port)));
    let key = match key {
        None => return Err(AppError::Msg(format!("No pending host key for {host}:{port} — connect (or Test SSH) first"))),
        Some(None) => {
            return Err(AppError::Msg(format!(
                "The host key for {host}:{port} changed after its fingerprint was shown — not trusted (possible man-in-the-middle). Connect again and check the new fingerprint."
            )))
        }
        Some(Some(k)) => k,
    };
    match known_hosts_path() {
        Some(p) => russh::keys::known_hosts::learn_known_hosts_path(host, port, &key, p),
        None => russh::keys::known_hosts::learn_known_hosts(host, port, &key),
    }
    .map_err(|e| AppError::Msg(format!("Could not write known_hosts: {e}")))?;
    Ok(format!("Trusted {host}:{port} ({})", key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256)))
}
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct Client {
    host: String,
    port: u16,
    /// Reason the server key check failed (for the connect error message)
    reject: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    /// Based on ~/.ssh/known_hosts: pass on match, reject if the key changed, stop and show the fingerprint for unknown servers
    /// (it used to add them without asking → the first connection could hand the password to a MITM)
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            *self.reject.lock().unwrap() = Some("SSH certificate host keys are not supported yet".into());
            return Ok(false);
        };
        let custom = known_hosts_path();
        let checked = match &custom {
            Some(p) => russh::keys::check_known_hosts_path(&self.host, self.port, key, p),
            None => russh::keys::check_known_hosts(&self.host, self.port, key),
        };
        match checked {
            Ok(true) => Ok(true),
            Ok(false) => {
                let fp = key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256);
                remember_pending(&self.host, self.port, key);
                *self.reject.lock().unwrap() = Some(format!(
                    "{UNKNOWN_HOST} for {}:{} — {} {fp}. Check that it matches the server, then choose \"Trust host key\".",
                    self.host,
                    self.port,
                    key.algorithm().as_str()
                ));
                Ok(false)
            }
            Err(e) => {
                *self.reject.lock().unwrap() =
                    Some(format!("Host key verification failed for {}:{} — {e} (check ~/.ssh/known_hosts)", self.host, self.port));
                Ok(false)
            }
        }
    }
}

pub struct Tunnel {
    pub local_port: u16,
    keep: Keep,
}

/// What forwards the port: the SSH accept loop and its session, or the aws CLI process (killed when dropped)
enum Keep {
    Ssh(JoinHandle<()>, Arc<Session>),
    Process(Box<crate::ssm::Process>),
}

impl Tunnel {
    pub(crate) fn process(local_port: u16, p: crate::ssm::Process) -> Self {
        Self { local_port, keep: Keep::Process(Box::new(p)) }
    }

    /// Still forwarding: the SSH accept loop runs and its session is open (the accept loop outlives a session that keepalive,
    /// sleep or a network change ended), or the aws CLI hasn't exited (SSM idle timeout, network, session ended)
    pub fn alive(&self) -> bool {
        match &self.keep {
            Keep::Ssh(t, session) => !t.is_finished() && !session.is_closed(),
            Keep::Process(p) => p.alive(),
        }
    }

    /// AWS SSM: taken before connecting through it, to tell whether the instance couldn't reach the database (ssm::connect_error)
    pub(crate) fn ssm_mark(&self) -> Option<crate::ssm::Mark> {
        match &self.keep {
            Keep::Ssh(..) => None,
            Keep::Process(p) => Some(p.mark()),
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Keep::Ssh(t, _) = &self.keep {
            t.abort();
        }
    }
}

pub(crate) fn expand_home(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => path.to_string(),
    }
}

/// The SSH session to the data source's SSH host, and the ProxyJump sessions it runs through (they live as long as it does)
struct Session {
    last: client::Handle<Client>,
    jumps: Vec<client::Handle<Client>>,
}

impl Session {
    fn is_closed(&self) -> bool {
        self.last.is_closed() || self.jumps.iter().any(|j| j.is_closed())
    }

    async fn disconnect(&self) {
        let _ = self.last.disconnect(russh::Disconnect::ByApplication, "", "en").await;
        for j in self.jumps.iter().rev() {
            let _ = j.disconnect(russh::Disconnect::ByApplication, "", "en").await;
        }
    }
}

/// The SSH servers on the way to the data source's SSH host, from ~/.ssh/config (crate::ssh_config): its HostName, User, Port,
/// IdentityFile, IdentityAgent and ProxyJump hosts. The source's own port wins unless it is the default 22, its user unless it is empty
fn route(ssh: &SshConfig) -> AppResult<Vec<crate::ssh_config::Hop>> {
    let home = dirs::home_dir().unwrap_or_default();
    let local = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let port = (ssh.port != 22).then_some(ssh.port);
    crate::ssh_config::SshConfigFile::load()?.route(&ssh.host, port, Some(&ssh.user), &home, &local)
}

/// Tests only the SSH login (Test Connection on the SSH/SSL tab).
pub async fn test_login(ssh: &SshConfig, secret: Option<&str>) -> AppResult<String> {
    let (session, hops) = login(ssh, secret).await?;
    session.disconnect().await;
    let (last, jumps) = hops.split_last().expect("a route ends at the SSH host");
    let via = if jumps.is_empty() { String::new() } else { format!(" via {}", jumps.iter().map(|h| h.host.as_str()).collect::<Vec<_>>().join(" → ")) };
    Ok(format!("SSH connection OK: {}@{}:{}{via} (host key verified)", last.user, last.host, last.port))
}

/// The data source's own credential for its SSH host. None (no key file, no password) = ssh-agent, then the IdentityFile keys, as `ssh` does
enum Own<'a> {
    Password(&'a str),
    Key(String),
}

async fn login(ssh: &SshConfig, secret: Option<&str>) -> AppResult<(Session, Vec<crate::ssh_config::Hop>)> {
    let hops = route(ssh)?;
    let own = match ssh.auth {
        SshAuth::Key if !ssh.key_path.trim().is_empty() => Some(Own::Key(expand_home(ssh.key_path.trim()))),
        SshAuth::Key => None,
        SshAuth::Password => secret.filter(|s| !s.is_empty()).map(Own::Password),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    });
    let wait = std::time::Duration::from_secs(15);
    let mut sessions: Vec<client::Handle<Client>> = Vec::new();
    for (i, hop) in hops.iter().enumerate() {
        let reject = Arc::new(Mutex::new(None));
        let handler = Client { host: hop.host.clone(), port: hop.port, reject: reject.clone() };
        let failed = |e: russh::Error| AppError::Msg(reject.lock().unwrap().take().unwrap_or_else(|| format!("SSH connect to {}:{} failed: {e}", hop.host, hop.port)));
        let timeout = |_| AppError::Msg(format!("SSH connect timeout: {}:{}", hop.host, hop.port));
        // The first hop over TCP; each next one through a direct-tcpip channel of the one before (ProxyJump)
        let mut session = match sessions.last() {
            None => tokio::time::timeout(wait, client::connect(config.clone(), (hop.host.as_str(), hop.port), handler)).await.map_err(timeout)?.map_err(failed)?,
            Some(prev) => {
                let ch = tokio::time::timeout(wait, prev.channel_open_direct_tcpip(hop.host.clone(), hop.port as u32, "127.0.0.1", 0))
                    .await
                    .map_err(timeout)?
                    .map_err(|e| AppError::Msg(format!("SSH jump host {} cannot reach {}:{} ({e})", hops[i - 1].host, hop.host, hop.port)))?;
                tokio::time::timeout(wait, client::connect_stream(config.clone(), ch.into_stream(), handler)).await.map_err(timeout)?.map_err(failed)?
            }
        };
        // The data source's password / key go only to its own SSH host, never to a jump host
        let last = i + 1 == hops.len();
        authenticate(&mut session, hop, own.as_ref().filter(|_| last), secret.filter(|_| last)).await?;
        sessions.push(session);
    }
    let last = sessions.pop().expect("a route ends at the SSH host");
    Ok((Session { last, jumps: sessions }, hops))
}

async fn authenticate(session: &mut client::Handle<Client>, hop: &crate::ssh_config::Hop, own: Option<&Own<'_>>, secret: Option<&str>) -> AppResult<()> {
    let auth_err = |e: &dyn std::fmt::Display| AppError::Msg(format!("SSH auth error ({}@{}): {e}", hop.user, hop.host));
    let hash = session.best_supported_rsa_hash().await.ok().flatten().flatten();
    let secret = secret.filter(|s| !s.is_empty());
    match own {
        Some(Own::Password(p)) => {
            return match session.authenticate_password(&hop.user, *p).await.map_err(|e| auth_err(&e))?.success() {
                true => Ok(()),
                false => Err(auth_failed(hop, &["the password".into()], "")),
            };
        }
        Some(Own::Key(path)) => {
            let key = russh::keys::load_secret_key(path, secret).map_err(|e| AppError::Msg(format!("Cannot load private key {path}: {e}")))?;
            return match session.authenticate_publickey(&hop.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(|e| auth_err(&e))?.success() {
                true => Ok(()),
                false => Err(auth_failed(hop, &[format!("the key {path}")], "")),
            };
        }
        None => {}
    }
    let mut tried = Vec::new();
    let mut hint = String::new();
    // ssh-agent first (keys with a passphrase are usually there), then the IdentityFile keys
    #[cfg(unix)]
    {
        use crate::ssh_config::Agent;
        let sock = match &hop.agent {
            Agent::Off => None,
            Agent::Socket(p) => Some(p.clone()),
            Agent::Env => {
                let s = std::env::var_os("SSH_AUTH_SOCK").map(std::path::PathBuf::from);
                if s.is_none() {
                    hint.push_str(" SSH_AUTH_SOCK isn't set, so no ssh-agent was asked.");
                }
                s
            }
        };
        if let Some(sock) = sock {
            match russh::keys::agent::client::AgentClient::connect_uds(&sock).await {
                Ok(mut agent) => {
                    let keys: Vec<_> = agent
                        .request_identities()
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|i| match i {
                            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => Some(key),
                            russh::keys::agent::AgentIdentity::Certificate { .. } => None,
                        })
                        .collect();
                    tried.push(format!("ssh-agent ({} key{})", keys.len(), if keys.len() == 1 { "" } else { "s" }));
                    for key in keys {
                        if session.authenticate_publickey_with(&hop.user, key, hash, &mut agent).await.map_err(|e| auth_err(&e))?.success() {
                            return Ok(());
                        }
                    }
                }
                Err(e) => hint.push_str(&format!(" The ssh-agent at {} didn't answer ({e}).", sock.display())),
            }
        }
    }
    for path in hop.identity_files.iter().filter(|p| p.exists()) {
        let shown = path.display().to_string();
        let key = match russh::keys::load_secret_key(path, secret) {
            Ok(k) => k,
            Err(russh::keys::Error::KeyIsEncrypted) => {
                hint.push_str(&format!(" {shown} needs a passphrase: add it to ssh-agent (ssh-add {shown})."));
                continue;
            }
            Err(e) => {
                hint.push_str(&format!(" Cannot load {shown}: {e}."));
                continue;
            }
        };
        tried.push(shown);
        if session.authenticate_publickey(&hop.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(|e| auth_err(&e))?.success() {
            return Ok(());
        }
    }
    if tried.is_empty() {
        tried.push("no key (no ssh-agent, no IdentityFile, no ~/.ssh/id_ed25519 · id_ecdsa · id_rsa)".into());
    }
    Err(auth_failed(hop, &tried, &hint))
}

fn auth_failed(hop: &crate::ssh_config::Hop, tried: &[String], hint: &str) -> AppError {
    let case = if hop.user.chars().any(|c| c.is_uppercase()) { " User names are case-sensitive (e.g. 'ubuntu', not 'Ubuntu')." } else { "" };
    AppError::Msg(format!(
        "SSH authentication failed for {}@{} — tried {}.{hint}{case} Check the user name, or set a key file or password for the SSH tunnel.",
        hop.user,
        hop.host,
        tried.join(", ")
    ))
}

pub async fn open(ssh: &SshConfig, secret: Option<&str>, remote_host: &str, remote_port: u16) -> AppResult<Tunnel> {
    let (session, hops) = login(ssh, secret).await?;
    let last = hops.last().expect("a route ends at the SSH host");

    // First check that the bastion can actually reach the DB address (otherwise you only see "connection reset")
    match session.last.channel_open_direct_tcpip(remote_host, remote_port as u32, "127.0.0.1", 0).await {
        Ok(ch) => {
            let _ = ch.close().await;
        }
        Err(e) => {
            return Err(AppError::Msg(format!(
                "SSH login OK, but {}@{} cannot reach {remote_host}:{remote_port} ({e}). \
                 Set the database host/port on the General tab as seen from the SSH server.",
                last.user, last.host
            )));
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let local_port = listener.local_addr()?.port();
    let session = Arc::new(session);
    let (rhost, rport) = (remote_host.to_string(), remote_port);
    let forward = session.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut tcp, peer)) = listener.accept().await else { break };
            let session = forward.clone();
            let rhost = rhost.clone();
            tokio::spawn(async move {
                match session.last.channel_open_direct_tcpip(rhost, rport as u32, peer.ip().to_string(), peer.port() as u32).await {
                    Ok(ch) => {
                        let mut stream = ch.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
                    }
                    Err(e) => crate::log!("[qurry] ssh channel open failed: {e}"),
                }
            });
        }
    });
    Ok(Tunnel { local_port, keep: Keep::Ssh(task, session) })
}

#[cfg(test)]
mod tests {
    /// If a different key arrives after the fingerprint was shown (MITM), it isn't added
    #[test]
    fn pending_key_swap_is_refused() {
        let k1 = russh::keys::PublicKey::from_openssh("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIhyj/8bbpg1Zf+HsFbmlTYF7gtrlPjwARofRnF50jlT").unwrap();
        let k2 = russh::keys::PublicKey::from_openssh("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJJn7OrkIc5LbrfWKCXp2+paFsn4xCxkpDR06FUpA0z4").unwrap();
        remember_pending("swap.test", 2222, &k1);
        remember_pending("swap.test", 2222, &k1); // Reconnecting with the same key is fine
        remember_pending("swap.test", 2222, &k2);
        let err = trust_host_key("swap.test", 2222).unwrap_err().to_string();
        assert!(err.contains("changed"), "{err}");
    }

    use super::*;
    use crate::db::connect_via;
    use crate::store::{ConnectionConfig, DbKind};

    /// Connects via a docker sshd (localhost:2222) → pg container (dbtool-pg:5432).
    /// Run: QURRY_SSH_TEST=1 cargo test --lib tunnel -- --ignored
    #[tokio::test]
    #[ignore]
    async fn ssh_tunnel_password_and_key() {
        let kh = std::env::temp_dir().join(format!("dbtool-kh-{}", uuid::Uuid::new_v4()));
        std::fs::write(&kh, "").unwrap();
        unsafe { std::env::set_var("QURRY_KNOWN_HOSTS", &kh) }; // Used only in a single test
        let cfg = ConnectionConfig {
            id: "t".into(), name: "t".into(), kind: DbKind::Postgres, host: "dbtool-pg".into(), port: 5432,
            user: "postgres".into(), database: "postgres".into(), color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        let mut ssh = SshConfig { enabled: true, host: "localhost".into(), port: 2222, user: "root".into(), auth: SshAuth::Password, key_path: String::new() };

        // 0) Unknown server: show the fingerprint and stop → once the user trusts it, it passes from then on
        let first = test_login(&ssh, Some("sshpw")).await.expect_err("unknown host must stop").to_string();
        println!("{first}");
        assert!(first.contains(UNKNOWN_HOST) && first.contains("SHA256:"), "{first}");
        assert!(std::fs::read_to_string(&kh).unwrap().is_empty(), "not learned before the user trusts it");
        println!("{}", trust_host_key(&ssh.host, ssh.port).expect("trust"));
        // SSH login only (SSH tab test) — must succeed regardless of the DB address
        let ok = test_login(&ssh, Some("sshpw")).await.expect("ssh-only login");
        println!("{ok}");
        // Message when the bastion can't reach the DB address
        let unreachable = open(&ssh, Some("sshpw"), "localhost", 54325).await;
        println!("unreachable: {}", unreachable.as_ref().err().unwrap());
        assert!(unreachable.err().unwrap().to_string().contains("SSH login OK"));

        // 1) Password auth
        let t = open(&ssh, Some("sshpw"), &cfg.host, cfg.port).await.expect("tunnel");
        let pool = connect_via(&cfg, Some("pgpw"), Some(t.local_port)).await.expect("pg via tunnel");
        let v = pool.server_version().await.unwrap();
        println!("via password tunnel: {v}");
        assert!(v.starts_with("PostgreSQL 16"));
        // Several connections at once (pool)
        let r = pool.run_query("select count(*) from generate_series(1,1000)", 10).await.unwrap();
        assert_eq!(r.rows[0][0], serde_json::Value::from(1000));
        assert!(std::fs::read_to_string(&kh).unwrap().contains("2222"), "host key trusted");
        pool.close().await;
        drop(t);

        // 2) Wrong password
        let bad = open(&ssh, Some("wrong"), &cfg.host, cfg.port).await;
        assert!(bad.is_err());
        println!("bad password: {}", bad.err().unwrap());

        // 3) Private key auth
        ssh.auth = SshAuth::Key;
        ssh.key_path = std::env::var("QURRY_SSH_TEST_KEY").unwrap();
        let t = open(&ssh, None, &cfg.host, cfg.port).await.expect("key tunnel");
        let pool = connect_via(&cfg, Some("pgpw"), Some(t.local_port)).await.unwrap();
        println!("via key tunnel: {}", pool.server_version().await.unwrap());
        pool.close().await;

        // 4) Reject when the host key changes
        std::fs::write(&kh, "[localhost]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n").unwrap();
        let mitm = open(&ssh, None, &cfg.host, cfg.port).await;
        println!("changed host key: {:?}", mitm.as_ref().err().map(|e| e.to_string()));
        assert!(mitm.is_err());
        let _ = std::fs::remove_file(&kh);
    }

    /// An SSH server in this process: password "pw", direct-tcpip forwarded to `to`. Counts logins
    struct Sshd {
        to: std::net::SocketAddr,
        logins: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl russh::server::Handler for Sshd {
        type Error = russh::Error;

        async fn auth_password(&mut self, _user: &str, password: &str) -> Result<russh::server::Auth, Self::Error> {
            if password != "pw" {
                return Ok(russh::server::Auth::reject());
            }
            self.logins.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(russh::server::Auth::Accept)
        }

        async fn channel_open_direct_tcpip(
            &mut self,
            channel: russh::Channel<russh::server::Msg>,
            _host: &str,
            _port: u32,
            _from: &str,
            _from_port: u32,
            reply: russh::server::ChannelOpenHandle,
            _session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            let to = self.to;
            tokio::spawn(async move {
                if let Ok(mut tcp) = tokio::net::TcpStream::connect(to).await {
                    let _ = tokio::io::copy_bidirectional(&mut tcp, &mut channel.into_stream()).await;
                }
            });
            Ok(())
        }
    }

    /// Port, login count, and the running sessions
    async fn sshd(to: std::net::SocketAddr, known_hosts: &std::path::Path) -> (u16, Arc<std::sync::atomic::AtomicUsize>, Arc<Mutex<Vec<russh::server::Handle>>>) {
        let key = russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7; 32]));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        russh::keys::known_hosts::learn_known_hosts_path("127.0.0.1", port, key.public_key(), known_hosts).unwrap();
        let config = Arc::new(russh::server::Config { keys: vec![key], auth_rejection_time: std::time::Duration::ZERO, ..Default::default() });
        let logins = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sessions = Arc::new(Mutex::new(Vec::new()));
        let (l, s) = (logins.clone(), sessions.clone());
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                if let Ok(running) = russh::server::run_stream(config.clone(), tcp, Sshd { to, logins: l.clone() }).await {
                    s.lock().unwrap().push(running.handle());
                    tokio::spawn(running);
                }
            }
        });
        (port, logins, sessions)
    }

    /// The SSH tunnel is shared by the databases of a source. A database that fails (missing) fails alone. When the SSH session
    /// ends (sleep, network change) while the accept loop still runs, the tunnel counts as stopped and the next database opens
    /// a new one instead of connecting through the dead session.
    /// Run on its own (it sets QURRY_KNOWN_HOSTS): QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres cargo test -p qurry-core shared_ssh_tunnel -- --ignored
    #[tokio::test]
    #[ignore]
    async fn shared_ssh_tunnel_survives_a_failed_db_and_reopens_after_the_session_ends() {
        let pg = crate::testenv::pg().expect("QURRY_TEST_PG").cfg;
        let to: std::net::SocketAddr = format!("{}:{}", pg.host, pg.port).parse().unwrap();
        let kh = tempfile::NamedTempFile::new().unwrap();
        unsafe { std::env::set_var("QURRY_KNOWN_HOSTS", kh.path()) };
        let (port, logins, sessions) = sshd(to, kh.path()).await;
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "ssh", "kind": "postgres", "host": "db.internal", "port": 5432, "user": &pg.user, "database": &pg.database,
            "ssh": { "enabled": true, "host": "127.0.0.1", "port": port, "user": "me", "auth": "password", "keyPath": "" } })).unwrap();
        let id = st.save_connection(c, None, Some("pw".into())).await.unwrap().id;
        let logins = || logins.load(std::sync::atomic::Ordering::SeqCst);

        // A missing database opened together with the default one: only it fails, through the one SSH session
        let missing = format!("{id}::qurry_ssh_no_such_db");
        let (bad, good) = tokio::join!(crate::app::ensure_pool(&st, &missing), crate::app::ensure_pool(&st, &id));
        let bad = bad.err().expect("missing db").to_string();
        assert!(bad.contains("does not exist"), "{bad}");
        assert!(good.expect("the default db connects").server_version().await.is_ok());
        assert_eq!(logins(), 1);
        assert!(crate::app::ensure_pool(&st, &missing).await.is_err());
        assert_eq!(logins(), 1, "the tunnel stayed open for the next try");

        // The session ends (as when keepalive gives up); the accept loop doesn't notice
        let ended: Vec<_> = sessions.lock().unwrap().drain(..).collect();
        for s in ended {
            let _ = s.disconnect(russh::Disconnect::ByApplication, String::new(), "en".into()).await;
        }
        let alive = || async { st.tunnels.read().await.get(&id).unwrap().alive() };
        for _ in 0..50 {
            if !alive().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(!alive().await, "a closed session is a stopped tunnel");
        let other = tokio::time::timeout(std::time::Duration::from_secs(20), crate::app::ensure_pool(&st, &format!("{id}::template1")))
            .await
            .expect("reopening doesn't hang")
            .expect("another db reopens the tunnel");
        assert!(other.server_version().await.is_ok());
        assert_eq!(logins(), 2);
        st.disconnect(id).await.unwrap();
    }

    /// An SSH server in this process that takes only one public key, and forwards direct-tcpip to the address asked for (a jump host).
    /// Records "user@port" per login
    #[derive(Clone)]
    struct KeySshd {
        key: russh::keys::PublicKey,
        port: u16,
        logins: Arc<Mutex<Vec<String>>>,
    }

    impl russh::server::Handler for KeySshd {
        type Error = russh::Error;

        async fn auth_publickey(&mut self, user: &str, key: &russh::keys::PublicKey) -> Result<russh::server::Auth, Self::Error> {
            if *key != self.key {
                return Ok(russh::server::Auth::reject());
            }
            self.logins.lock().unwrap().push(format!("{user}@{}", self.port));
            Ok(russh::server::Auth::Accept)
        }

        async fn channel_open_direct_tcpip(
            &mut self,
            channel: russh::Channel<russh::server::Msg>,
            host: &str,
            port: u32,
            _from: &str,
            _from_port: u32,
            reply: russh::server::ChannelOpenHandle,
            _session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            let to = format!("{host}:{port}");
            reply.accept().await;
            tokio::spawn(async move {
                if let Ok(mut tcp) = tokio::net::TcpStream::connect(to).await {
                    let _ = tokio::io::copy_bidirectional(&mut tcp, &mut channel.into_stream()).await;
                }
            });
            Ok(())
        }
    }

    async fn key_sshd(seed: u8, client: &russh::keys::PublicKey, logins: Arc<Mutex<Vec<String>>>) -> (u16, russh::keys::PublicKey) {
        let key = russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32]));
        let host_key = key.public_key().clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = Arc::new(russh::server::Config { keys: vec![key], auth_rejection_time: std::time::Duration::ZERO, ..Default::default() });
        let h = KeySshd { key: client.clone(), port, logins };
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                if let Ok(running) = russh::server::run_stream(config.clone(), tcp, h.clone()).await {
                    tokio::spawn(running);
                }
            }
        });
        (port, host_key)
    }

    /// ssh-agent and ~/.ssh/config: a Host alias with HostName, User, Port and ProxyJump through a jump host, both logged in with the
    /// agent's key (the data source has no key file and no password). An unknown jump host key stops the connect, and trusting the
    /// data source's SSH host trusts that jump host. A wrong ProxyCommand is refused.
    /// Run on its own (it sets QURRY_KNOWN_HOSTS and QURRY_SSH_CONFIG): cargo test -p qurry-core ssh_agent_and_proxy_jump -- --ignored
    #[cfg(unix)]
    #[tokio::test]
    #[ignore]
    async fn ssh_agent_and_proxy_jump_from_ssh_config() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir().unwrap();
        // The agent, holding the only key the servers take
        let client_key = russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[9; 32]));
        let sock = dir.path().join("agent.sock");
        let agent_listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let incoming = futures::stream::unfold(agent_listener, |l| async move { Some((l.accept().await.map(|(s, _)| s), l)) });
        tokio::spawn(russh::keys::agent::server::serve(Box::pin(incoming), ()));
        let mut agent = russh::keys::agent::client::AgentClient::connect_uds(&sock).await.unwrap();
        agent.add_identity(&client_key, &[]).await.unwrap();

        let logins = Arc::new(Mutex::new(Vec::new()));
        let (jump, jump_key) = key_sshd(11, client_key.public_key(), logins.clone()).await;
        let (target, target_key) = key_sshd(12, client_key.public_key(), logins.clone()).await;
        // What the tunnel reaches: an echo server
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        let kh = dir.path().join("known_hosts");
        russh::keys::known_hosts::learn_known_hosts_path("127.0.0.1", target, &target_key, &kh).unwrap();
        let conf = dir.path().join("config");
        let s = sock.display();
        std::fs::write(&conf, format!("Host qurry-target\n  HostName 127.0.0.1\n  Port {target}\n  User deploy\n  ProxyJump jumper\n  IdentityAgent {s}\nHost jumper\n  HostName 127.0.0.1\n  Port {jump}\n  User hop\n  IdentityAgent {s}\nHost qurry-proxycmd\n  ProxyCommand nc %h %p\n")).unwrap();
        unsafe {
            std::env::set_var("QURRY_KNOWN_HOSTS", &kh);
            std::env::set_var("QURRY_SSH_CONFIG", &conf);
        }
        let ssh = SshConfig { enabled: true, host: "qurry-target".into(), port: 22, user: String::new(), auth: SshAuth::Key, key_path: String::new() };

        // The jump host's key is unknown: stop and show it; trusting the data source's SSH host trusts the jump host
        let e = test_login(&ssh, None).await.unwrap_err().to_string();
        assert!(e.contains(UNKNOWN_HOST) && e.contains(&format!("127.0.0.1:{jump}")), "{e}");
        assert!(trust_host_key("qurry-target", 22).unwrap().contains(&format!("127.0.0.1:{jump}")));
        assert!(std::fs::read_to_string(&kh).unwrap().contains(&jump.to_string()));
        let _ = jump_key;

        let ok = test_login(&ssh, None).await.unwrap();
        assert!(ok.contains(&format!("deploy@127.0.0.1:{target} via 127.0.0.1")), "{ok}");
        assert_eq!(logins.lock().unwrap().drain(..).collect::<Vec<_>>(), [format!("hop@{jump}"), format!("deploy@{target}")]);

        // A tunnel through both, to the echo server; a password-auth source with no password uses the agent too
        let pw = SshConfig { auth: SshAuth::Password, ..ssh.clone() };
        let t = open(&pw, None, "127.0.0.1", echo_port).await.unwrap();
        let mut c = tokio::net::TcpStream::connect(("127.0.0.1", t.local_port)).await.unwrap();
        c.write_all(b"through two hops").await.unwrap();
        let mut buf = [0u8; 16];
        c.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"through two hops");
        assert!(t.alive());
        drop(t);

        // No agent and no key: says what it tried
        unsafe { std::env::set_var("QURRY_SSH_CONFIG", dir.path().join("empty")) };
        let direct = SshConfig { host: "127.0.0.1".into(), port: target, user: "deploy".into(), ..ssh.clone() };
        let saved = std::env::var_os("SSH_AUTH_SOCK");
        unsafe { std::env::remove_var("SSH_AUTH_SOCK") };
        let e = test_login(&direct, None).await.unwrap_err().to_string();
        if let Some(s) = saved {
            unsafe { std::env::set_var("SSH_AUTH_SOCK", s) };
        }
        assert!(e.contains("SSH authentication failed for deploy@127.0.0.1") && e.contains("SSH_AUTH_SOCK"), "{e}");

        // ProxyCommand is refused before anything connects
        unsafe { std::env::set_var("QURRY_SSH_CONFIG", &conf) };
        let e = test_login(&SshConfig { host: "qurry-proxycmd".into(), ..ssh.clone() }, None).await.unwrap_err().to_string();
        assert!(e.contains("ProxyCommand isn't supported"), "{e}");
        unsafe { std::env::remove_var("QURRY_SSH_CONFIG") };
    }
}
