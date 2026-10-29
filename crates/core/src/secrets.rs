//! Password store: an encrypted file in the app config folder (to avoid keychain popups).
//! - secrets.json: { key: base64(nonce(12) || AES-256-GCM ciphertext) }
//! - secret.key  : 32-byte random key
//! Both files have permission 0600. Anyone reading the files as the same user can decrypt them (DBeaver level).
//! In settings, the master key can be moved to the OS keychain (macOS Keychain · Secret Service · Windows Credential Manager):
//! then a secret.key.keychain marker file remains instead of secret.key, and only the key is in the keychain (passwords stay in secrets.json).
use crate::error::{AppError, AppResult};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use rand::RngCore;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct SecretFile {
    path: PathBuf,
    dir: PathBuf,
    /// Keychain mode opens it when a password is first needed (the app starts regardless of the keychain; if it can't open, the reason)
    cipher: std::sync::OnceLock<Result<Aes256Gcm, String>>,
    vault: std::sync::Arc<dyn KeyVault + Send + Sync>,
}

/// Where the master key is kept
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyLocation {
    File,
    Keychain,
}

/// OS store holding the single master key (memory in tests)
pub trait KeyVault {
    fn get(&self, account: &str) -> AppResult<Option<Vec<u8>>>;
    fn set(&self, account: &str, key: &[u8]) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// OS keychain
pub struct OsKeychain;

const KEYCHAIN_SERVICE: &str = "Qurry";
const KEYCHAIN_MARKER: &str = "secret.key.keychain";

/// Keychain calls run on a separate thread (Linux Secret Service runs its own runtime inside, so calling it from a tokio thread hangs)
fn off_runtime<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| s.spawn(f).join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
}

impl KeyVault for OsKeychain {
    fn get(&self, account: &str) -> AppResult<Option<Vec<u8>>> {
        off_runtime(|| {
            let e = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| AppError::Msg(format!("System keychain: {e}")))?;
            match e.get_secret() {
                Ok(k) => Ok(Some(k)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(AppError::Msg(format!("System keychain: {e}"))),
            }
        })
    }
    fn set(&self, account: &str, key: &[u8]) -> AppResult<()> {
        off_runtime(|| keyring::Entry::new(KEYCHAIN_SERVICE, account).and_then(|e| e.set_secret(key)).map_err(|e| AppError::Msg(format!("System keychain: {e}"))))
    }
    fn delete(&self, account: &str) -> AppResult<()> {
        off_runtime(|| match keyring::Entry::new(KEYCHAIN_SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Msg(format!("System keychain: {e}"))),
        })
    }
}

/// New keychain entry name: separate per config folder (so MCP launched with --config-dir and tests don't mix)
fn new_keychain_account(dir: &Path) -> String {
    format!("password key — {}", fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()).display())
}

/// Keychain entry name in use: the one written to the marker file when moved (same entry even if the folder moves or is opened via another path)
fn keychain_account(dir: &Path) -> String {
    fs::read_to_string(dir.join(KEYCHAIN_MARKER)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| new_keychain_account(dir))
}

pub fn key_location(dir: &Path) -> KeyLocation {
    if dir.join(KEYCHAIN_MARKER).exists() { KeyLocation::Keychain } else { KeyLocation::File }
}

/// Move the master key (passwords are not re-encrypted: same key). When moving into the keychain, read it back to verify, then delete secret.key
pub fn move_key(dir: &Path, to: KeyLocation, vault: &dyn KeyVault) -> AppResult<()> {
    let (key_path, marker) = (dir.join("secret.key"), dir.join(KEYCHAIN_MARKER));
    match (key_location(dir), to) {
        (KeyLocation::File, KeyLocation::Keychain) => {
            let account = new_keychain_account(dir);
            let key = fs::read(&key_path)?;
            vault.set(&account, &key)?;
            if vault.get(&account)?.as_deref() != Some(key.as_slice()) {
                let _ = vault.delete(&account);
                return Err(AppError::Msg("The system keychain didn't keep the key — it stays in the file".into()));
            }
            write_private(&marker, account.as_bytes())?;
            // If the file can't be deleted, revert (with only the marker file left, the key would be looked up in two places)
            if let Err(e) = fs::remove_file(&key_path) {
                let _ = fs::remove_file(&marker);
                let _ = vault.delete(&account);
                return Err(e.into());
            }
        }
        (KeyLocation::Keychain, KeyLocation::File) => {
            let account = keychain_account(dir);
            let key = vault.get(&account)?.ok_or_else(|| AppError::Msg("The key is not in the system keychain".into()))?;
            write_private(&key_path, &key)?;
            fs::remove_file(&marker)?;
            vault.delete(&account)?;
        }
        _ => {}
    }
    Ok(())
}

/// Write an owner-only file: write to a temp file created as 0600 from the start, then rename (never open as 0644 even briefly, and no partial writes)
pub(crate) fn write_private(path: &Path, data: &[u8]) -> AppResult<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        private_dir(dir)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Config folder: 0700 so only the owner can enter
pub(crate) fn private_dir(dir: &Path) -> AppResult<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

impl SecretFile {
    pub fn open(dir: &Path) -> AppResult<Self> {
        Self::open_with(dir, std::sync::Arc::new(OsKeychain))
    }

    pub fn open_with(dir: &Path, vault: std::sync::Arc<dyn KeyVault + Send + Sync>) -> AppResult<Self> {
        private_dir(dir)?;
        let me = |cipher: std::sync::OnceLock<Result<Aes256Gcm, String>>| Self { path: dir.join("secrets.json"), dir: dir.to_path_buf(), cipher, vault: vault.clone() };
        // Keychain mode: don't open it now (the app starts even if locked, over SSH, or headless)
        if key_location(dir) == KeyLocation::Keychain {
            return Ok(me(std::sync::OnceLock::new()));
        }
        let key_path = dir.join("secret.key");
        let secrets_path = dir.join("secrets.json");
        let key: Vec<u8> = match fs::read(&key_path) {
            Ok(k) if k.len() == 32 => k,
            // If it exists but can't be read or has the wrong length, don't create a new one (overwriting would make stored passwords undecryptable forever)
            Ok(_) => return Err(AppError::Msg(format!("{} is corrupted — restore it from a backup, or delete it and secrets.json to start over", key_path.display()))),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(AppError::Msg(format!("Cannot read {}: {e}", key_path.display()))),
            Err(_) => {
                // If only the password file is left without a key, move it aside instead of deleting it
                if secrets_path.exists() {
                    let _ = fs::rename(&secrets_path, dir.join(format!("secrets.json.orphaned-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))));
                }
                let mut k = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut k);
                write_private(&key_path, &k)?;
                k
            }
        };
        Ok(me(std::sync::OnceLock::from(Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key))))))
    }

    /// Read the key now (before moving the key: so this store keeps using the same key after the move)
    pub fn ensure_key(&self) -> AppResult<()> {
        self.cipher().map(|_| ())
    }

    /// Encryption key: in keychain mode, read from the keychain on first call (if it can't be read, the reason; no new key is created — stored passwords would become undecryptable)
    fn cipher(&self) -> AppResult<&Aes256Gcm> {
        self.cipher
            .get_or_init(|| match self.vault.get(&keychain_account(&self.dir)) {
                Ok(Some(k)) if k.len() == 32 => Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k))),
                Ok(_) => Err(format!("The password key is not in the system keychain (see {}) — restore it, or delete that file and secrets.json to start over", self.dir.join(KEYCHAIN_MARKER).display())),
                Err(e) => Err(format!("{e} — allow Qurry to use the keychain (saved passwords can't be read until then)")),
            })
            .as_ref()
            .map_err(|e| AppError::Msg(e.clone()))
    }

    /// Empty list if missing. Error if unreadable or corrupt (treating it as empty and saving would wipe all other passwords)
    fn load(&self) -> AppResult<HashMap<String, String>> {
        match fs::read_to_string(&self.path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| AppError::Msg(format!("{} is damaged ({e}) — saved passwords can't be read. Restore it from a backup, or move it away to start over", self.path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(AppError::Msg(format!("Cannot read {}: {e}", self.path.display()))),
        }
    }

    /// File shared by GUI · TUI · MCP: locked while reading and rewriting (so concurrent saves don't lose one side)
    fn locked<T>(&self, f: impl FnOnce(&mut HashMap<String, String>) -> AppResult<(T, bool)>) -> AppResult<T> {
        let lock = fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.path.with_extension("json.lock"))?;
        lock.lock()?;
        let mut all = self.load()?;
        let (out, changed) = f(&mut all)?;
        if changed {
            write_private(&self.path, serde_json::to_string_pretty(&all)?.as_bytes())?;
        }
        Ok(out)
    }

    fn seal(&self, name: &str, v: &str) -> AppResult<String> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher()?
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: v.as_bytes(), aad: name.as_bytes() })
            .map_err(|_| AppError::Msg("encryption failed".into()))?;
        let mut raw = nonce.to_vec();
        raw.extend(ct);
        Ok(base64::engine::general_purpose::STANDARD.encode(raw))
    }

    /// (value, whether it was the old format)
    fn open_entry(&self, cipher: &Aes256Gcm, name: &str, enc: &str) -> Option<(String, bool)> {
        let raw = base64::engine::general_purpose::STANDARD.decode(enc).ok()?;
        if raw.len() < 12 {
            return None;
        }
        let (nonce, ct) = raw.split_at(12);
        // Bind the name as AAD (so ciphertext moved from another entry doesn't decrypt)
        if let Ok(pt) = cipher.decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: name.as_bytes() }) {
            return String::from_utf8(pt).ok().map(|s| (s, false));
        }
        // Old format (no AAD): re-saved in the new format as soon as it is read. This fallback is removed in 0.4 (by then all used entries will have been converted)
        String::from_utf8(cipher.decrypt(Nonce::from_slice(nonce), ct).ok()?).ok().map(|s| (s, true))
    }

    pub fn get(&self, name: &str) -> AppResult<Option<String>> {
        if !self.load()?.contains_key(name) {
            return Ok(None);
        }
        let cipher = self.cipher()?;
        self.locked(|all| {
            let Some((v, legacy)) = all.get(name).and_then(|enc| self.open_entry(cipher, name, enc)) else { return Ok((None, false)) };
            if legacy {
                all.insert(name.to_string(), self.seal(name, &v)?);
            }
            Ok((Some(v), legacy))
        })
    }

    pub fn set(&self, name: &str, value: Option<&str>) -> AppResult<()> {
        self.locked(|all| {
            match value {
                Some(v) => {
                    all.insert(name.to_string(), self.seal(name, v)?);
                }
                None => {
                    all.remove(name);
                }
            }
            Ok(((), true))
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.load().is_ok_and(|a| a.contains_key(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_permissions() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let s = SecretFile::open(&dir).unwrap();
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", Some("p@ss 한글")).unwrap();
        s.set("b#ssh", Some("x")).unwrap();
        // Must still decrypt after reopening (key file reused)
        let s2 = SecretFile::open(&dir).unwrap();
        assert_eq!(s2.get("a").unwrap().as_deref(), Some("p@ss 한글"));
        assert!(!std::fs::read_to_string(dir.join("secrets.json")).unwrap().contains("p@ss"));
        s2.set("a", None).unwrap();
        assert_eq!(s2.get("a").unwrap(), None);
        assert!(s2.contains("b#ssh"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in ["secrets.json", "secret.key"] {
                assert_eq!(std::fs::metadata(dir.join(f)).unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A corrupt file is not overwritten (so stored passwords aren't all lost)
    #[test]
    fn damaged_file_is_kept() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let s = SecretFile::open(&dir).unwrap();
        s.set("a", Some("1")).unwrap();
        std::fs::write(dir.join("secrets.json"), "{ \"a\": \"tru").unwrap();
        assert!(s.get("a").is_err());
        assert!(s.set("b", Some("2")).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("secrets.json")).unwrap(), "{ \"a\": \"tru");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Concurrent saves from GUI · TUI · MCP all survive
    #[test]
    fn concurrent_sets_keep_every_entry() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        SecretFile::open(&dir).unwrap();
        let hs: Vec<_> = (0..8)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let s = SecretFile::open(&dir).unwrap();
                    for i in 0..20 {
                        s.set(&format!("k{t}-{i}"), Some("v")).unwrap();
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        let s = SecretFile::open(&dir).unwrap();
        for t in 0..8 {
            for i in 0..20 {
                assert!(s.contains(&format!("k{t}-{i}")), "k{t}-{i}");
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// In-memory keychain (tests)
    #[derive(Default)]
    struct MemVault(std::sync::Mutex<HashMap<String, Vec<u8>>>);
    impl KeyVault for MemVault {
        fn get(&self, a: &str) -> AppResult<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(a).cloned())
        }
        fn set(&self, a: &str, k: &[u8]) -> AppResult<()> {
            self.0.lock().unwrap().insert(a.into(), k.to_vec());
            Ok(())
        }
        fn delete(&self, a: &str) -> AppResult<()> {
            self.0.lock().unwrap().remove(a);
            Ok(())
        }
    }

    /// Stored passwords still decrypt after moving the master key to the keychain · and back. If missing from the keychain, no new key is created
    #[test]
    fn master_key_moves_between_file_and_keychain() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let vault = std::sync::Arc::new(MemVault::default());
        SecretFile::open_with(&dir, vault.clone()).unwrap().set("a", Some("pw")).unwrap();
        move_key(&dir, KeyLocation::Keychain, &*vault).unwrap();
        assert_eq!(key_location(&dir), KeyLocation::Keychain);
        assert!(!dir.join("secret.key").exists());
        assert_eq!(SecretFile::open_with(&dir, vault.clone()).unwrap().get("a").unwrap().as_deref(), Some("pw"));
        // Opens even without a key in the keychain (the app starts); the reason is reported when a password is used. No new key is created
        let empty = SecretFile::open_with(&dir, std::sync::Arc::new(MemVault::default())).unwrap();
        assert!(empty.get("a").is_err() && empty.set("b", Some("x")).is_err());
        assert!(empty.contains("a"));
        assert!(!dir.join("secret.key").exists());
        move_key(&dir, KeyLocation::File, &*vault).unwrap();
        assert_eq!(key_location(&dir), KeyLocation::File);
        assert!(vault.0.lock().unwrap().is_empty());
        assert_eq!(SecretFile::open_with(&dir, vault.clone()).unwrap().get("a").unwrap().as_deref(), Some("pw"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The old format (no AAD) is converted to the new format once read (moved ciphertext no longer decrypts)
    #[test]
    fn legacy_entries_are_upgraded() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let s = SecretFile::open(&dir).unwrap();
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = s.cipher().unwrap().encrypt(Nonce::from_slice(&nonce), b"old".as_ref()).unwrap();
        let enc = base64::engine::general_purpose::STANDARD.encode([nonce.to_vec(), ct].concat());
        std::fs::write(dir.join("secrets.json"), serde_json::json!({ "a": enc }).to_string()).unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("old"));
        let now: HashMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("secrets.json")).unwrap()).unwrap();
        assert_ne!(now["a"], enc);
        // Moved under a different name, it doesn't decrypt
        std::fs::write(dir.join("secrets.json"), serde_json::json!({ "b": now["a"] }).to_string()).unwrap();
        assert_eq!(s.get("b").unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
