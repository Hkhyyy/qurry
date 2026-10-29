//! SSH tunnel: opens a local port and forwards incoming connections to the remote DB over SSH direct-tcpip channels.
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

/// Adds a server key whose fingerprint the user confirmed to known_hosts (passes from the next connection on)
pub fn trust_host_key(host: &str, port: u16) -> AppResult<String> {
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
    task: JoinHandle<()>,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn expand_home(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => path.to_string(),
    }
}

/// Tests only the SSH login (Test Connection on the SSH/SSL tab).
pub async fn test_login(ssh: &SshConfig, secret: Option<&str>) -> AppResult<String> {
    let session = login(ssh, secret).await?;
    let _ = session.disconnect(russh::Disconnect::ByApplication, "", "en").await;
    Ok(format!("SSH connection OK: {}@{}:{} (host key verified)", ssh.user, ssh.host, ssh.port))
}

async fn login(ssh: &SshConfig, secret: Option<&str>) -> AppResult<client::Handle<Client>> {
    let reject = Arc::new(Mutex::new(None));
    let handler = Client { host: ssh.host.clone(), port: ssh.port, reject: reject.clone() };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    });
    let mut session = tokio::time::timeout(std::time::Duration::from_secs(15), client::connect(config, (ssh.host.as_str(), ssh.port), handler))
        .await
        .map_err(|_| AppError::Msg(format!("SSH connect timeout: {}:{}", ssh.host, ssh.port)))?
        .map_err(|e| AppError::Msg(reject.lock().unwrap().take().unwrap_or_else(|| format!("SSH connect failed: {e}"))))?;

    let auth = match ssh.auth {
        SshAuth::Password => session
            .authenticate_password(&ssh.user, secret.unwrap_or_default())
            .await
            .map_err(|e| AppError::Msg(format!("SSH auth error: {e}")))?,
        SshAuth::Key => {
            let path = expand_home(&ssh.key_path);
            let key = russh::keys::load_secret_key(&path, secret.filter(|s| !s.is_empty()))
                .map_err(|e| AppError::Msg(format!("Cannot load private key {path}: {e}")))?;
            let hash = session.best_supported_rsa_hash().await.ok().flatten().flatten();
            session
                .authenticate_publickey(&ssh.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(|e| AppError::Msg(format!("SSH auth error: {e}")))?
        }
    };
    if !auth.success() {
        let hint = if ssh.user.chars().any(|c| c.is_uppercase()) { " — user names are case-sensitive (e.g. 'ubuntu', not 'Ubuntu')" } else { " — check the user name and key/password" };
        return Err(AppError::Msg(format!("SSH authentication failed for {}@{}{hint}", ssh.user, ssh.host)));
    }
    Ok(session)
}

pub async fn open(ssh: &SshConfig, secret: Option<&str>, remote_host: &str, remote_port: u16) -> AppResult<Tunnel> {
    let session = login(ssh, secret).await?;

    // First check that the bastion can actually reach the DB address (otherwise you only see "connection reset")
    match session.channel_open_direct_tcpip(remote_host, remote_port as u32, "127.0.0.1", 0).await {
        Ok(ch) => {
            let _ = ch.close().await;
        }
        Err(e) => {
            return Err(AppError::Msg(format!(
                "SSH login OK, but {}@{} cannot reach {remote_host}:{remote_port} ({e}). \
                 Set the database host/port on the General tab as seen from the SSH server.",
                ssh.user, ssh.host
            )));
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let local_port = listener.local_addr()?.port();
    let session = Arc::new(session);
    let (rhost, rport) = (remote_host.to_string(), remote_port);
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut tcp, peer)) = listener.accept().await else { break };
            let session = session.clone();
            let rhost = rhost.clone();
            tokio::spawn(async move {
                match session.channel_open_direct_tcpip(rhost, rport as u32, peer.ip().to_string(), peer.port() as u32).await {
                    Ok(ch) => {
                        let mut stream = ch.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
                    }
                    Err(e) => crate::log!("[qurry] ssh channel open failed: {e}"),
                }
            });
        }
    });
    Ok(Tunnel { local_port, task })
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
            user: "postgres".into(), database: "postgres".into(), color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let mut ssh = SshConfig { enabled: true, host: "localhost".into(), port: 2222, user: "root".into(), auth: SshAuth::Password, key_path: String::new() };

        // 0) Unknown server: show the fingerprint and stop → once the user trusts it, it passes from then on
        let first = test_login(&ssh, Some("sshpw")).await.err().expect("unknown host must stop").to_string();
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
}
