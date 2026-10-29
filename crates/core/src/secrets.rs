//! 비밀번호 저장소: 앱 설정 폴더의 암호화 파일 (키체인 팝업을 피하기 위해).
//! - secrets.json: { key: base64(nonce(12) || AES-256-GCM ciphertext) }
//! - secret.key  : 32바이트 랜덤 키
//! 두 파일 모두 권한 0600. 같은 사용자 권한으로 파일을 읽으면 복호화할 수 있다 (DBeaver 수준).
//! 설정에서 마스터 키를 OS 키체인 (macOS Keychain · Secret Service · Windows Credential Manager) 으로 옮길 수 있다:
//! 그러면 secret.key 대신 secret.key.keychain 표시 파일이 남고, 키 하나만 키체인에 있다 (비밀번호는 그대로 secrets.json).
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
    cipher: Aes256Gcm,
}

/// 마스터 키를 둔 곳
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyLocation {
    File,
    Keychain,
}

/// 마스터 키 하나를 넣어 두는 OS 저장소 (테스트는 메모리)
pub trait KeyVault {
    fn get(&self, account: &str) -> AppResult<Option<Vec<u8>>>;
    fn set(&self, account: &str, key: &[u8]) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// OS 키체인
pub struct OsKeychain;

const KEYCHAIN_SERVICE: &str = "Qurry";
const KEYCHAIN_MARKER: &str = "secret.key.keychain";

impl KeyVault for OsKeychain {
    fn get(&self, account: &str) -> AppResult<Option<Vec<u8>>> {
        let e = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| AppError::Msg(format!("System keychain: {e}")))?;
        match e.get_secret() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Msg(format!("System keychain: {e}"))),
        }
    }
    fn set(&self, account: &str, key: &[u8]) -> AppResult<()> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).and_then(|e| e.set_secret(key)).map_err(|e| AppError::Msg(format!("System keychain: {e}")))
    }
    fn delete(&self, account: &str) -> AppResult<()> {
        match keyring::Entry::new(KEYCHAIN_SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Msg(format!("System keychain: {e}"))),
        }
    }
}

/// 키체인 항목 이름: 설정 폴더마다 따로 (--config-dir 로 띄운 MCP · 테스트가 섞이지 않게)
fn keychain_account(dir: &Path) -> String {
    format!("password key — {}", fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()).display())
}

pub fn key_location(dir: &Path) -> KeyLocation {
    if dir.join(KEYCHAIN_MARKER).exists() { KeyLocation::Keychain } else { KeyLocation::File }
}

/// 마스터 키를 옮긴다 (비밀번호는 다시 암호화하지 않는다: 같은 키). 키체인에 넣으면 읽어 확인한 뒤 secret.key 를 지운다
pub fn move_key(dir: &Path, to: KeyLocation, vault: &dyn KeyVault) -> AppResult<()> {
    let (key_path, marker) = (dir.join("secret.key"), dir.join(KEYCHAIN_MARKER));
    let account = keychain_account(dir);
    match (key_location(dir), to) {
        (KeyLocation::File, KeyLocation::Keychain) => {
            let key = fs::read(&key_path)?;
            vault.set(&account, &key)?;
            if vault.get(&account)?.as_deref() != Some(key.as_slice()) {
                let _ = vault.delete(&account);
                return Err(AppError::Msg("The system keychain didn't keep the key — it stays in the file".into()));
            }
            write_private(&marker, account.as_bytes())?;
            fs::remove_file(&key_path)?;
        }
        (KeyLocation::Keychain, KeyLocation::File) => {
            let key = vault.get(&account)?.ok_or_else(|| AppError::Msg("The key is not in the system keychain".into()))?;
            write_private(&key_path, &key)?;
            fs::remove_file(&marker)?;
            vault.delete(&account)?;
        }
        _ => {}
    }
    Ok(())
}

/// 본인만 읽는 파일로 쓴다: 처음부터 0600 으로 만든 임시 파일에 쓰고 rename (잠깐이라도 0644 로 열려 있지 않게, 도중에 깨지지 않게)
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

/// 설정 폴더: 본인만 들어갈 수 있게 0700
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
        Self::open_with(dir, &OsKeychain)
    }

    pub fn open_with(dir: &Path, vault: &dyn KeyVault) -> AppResult<Self> {
        private_dir(dir)?;
        if key_location(dir) == KeyLocation::Keychain {
            // 키체인에 두기로 했으면 파일로 돌아가지 않는다 (새 키를 만들면 저장된 비밀번호를 못 푼다)
            let key = vault.get(&keychain_account(dir))?.filter(|k| k.len() == 32).ok_or_else(|| {
                AppError::Msg(format!("The password key is not in the system keychain (see {}) — allow Qurry in the keychain, or restore the key", dir.join(KEYCHAIN_MARKER).display()))
            })?;
            return Ok(Self { path: dir.join("secrets.json"), cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)) });
        }
        let key_path = dir.join("secret.key");
        let secrets_path = dir.join("secrets.json");
        let key: Vec<u8> = match fs::read(&key_path) {
            Ok(k) if k.len() == 32 => k,
            // 있는데 못 읽거나 길이가 틀리면 새로 만들지 않는다 (덮어쓰면 저장된 비밀번호를 영영 못 푼다)
            Ok(_) => return Err(AppError::Msg(format!("{} is corrupted — restore it from a backup, or delete it and secrets.json to start over", key_path.display()))),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(AppError::Msg(format!("Cannot read {}: {e}", key_path.display()))),
            Err(_) => {
                // 키 없이 비밀번호 파일만 남아 있으면 지우지 말고 옆으로 치워 둔다
                if secrets_path.exists() {
                    let _ = fs::rename(&secrets_path, dir.join(format!("secrets.json.orphaned-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))));
                }
                let mut k = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut k);
                write_private(&key_path, &k)?;
                k
            }
        };
        Ok(Self { path: dir.join("secrets.json"), cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)) })
    }

    /// 없으면 빈 목록. 읽을 수 없거나 깨졌으면 오류 (빈 목록으로 보고 저장하면 나머지 비밀번호가 다 사라진다)
    fn load(&self) -> AppResult<HashMap<String, String>> {
        match fs::read_to_string(&self.path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| AppError::Msg(format!("{} is damaged ({e}) — saved passwords can't be read. Restore it from a backup, or move it away to start over", self.path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(AppError::Msg(format!("Cannot read {}: {e}", self.path.display()))),
        }
    }

    /// GUI · TUI · MCP 가 함께 쓰는 파일: 읽고 고쳐 쓰는 동안 잠근다 (동시에 저장해도 한쪽이 사라지지 않게)
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
            .cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: v.as_bytes(), aad: name.as_bytes() })
            .map_err(|_| AppError::Msg("encryption failed".into()))?;
        let mut raw = nonce.to_vec();
        raw.extend(ct);
        Ok(base64::engine::general_purpose::STANDARD.encode(raw))
    }

    /// (값, 예전 형식이었는지)
    fn open_entry(&self, name: &str, enc: &str) -> Option<(String, bool)> {
        let raw = base64::engine::general_purpose::STANDARD.decode(enc).ok()?;
        if raw.len() < 12 {
            return None;
        }
        let (nonce, ct) = raw.split_at(12);
        // 이름을 AAD 로 묶어 둔다 (다른 항목의 암호문을 옮겨 붙이면 풀리지 않게)
        if let Ok(pt) = self.cipher.decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: name.as_bytes() }) {
            return String::from_utf8(pt).ok().map(|s| (s, false));
        }
        // 예전 형식 (AAD 없음): 읽으면 바로 새 형식으로 다시 저장한다
        String::from_utf8(self.cipher.decrypt(Nonce::from_slice(nonce), ct).ok()?).ok().map(|s| (s, true))
    }

    pub fn get(&self, name: &str) -> AppResult<Option<String>> {
        if !self.load()?.contains_key(name) {
            return Ok(None);
        }
        self.locked(|all| {
            let Some((v, legacy)) = all.get(name).and_then(|enc| self.open_entry(name, enc)) else { return Ok((None, false)) };
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
        // 다시 열어도(키 파일 재사용) 복호화돼야 함
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

    /// 깨진 파일은 덮어쓰지 않는다 (저장된 비밀번호가 다 사라지지 않게)
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

    /// GUI · TUI · MCP 가 동시에 저장해도 다 남는다
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

    /// 메모리 키체인 (테스트)
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

    /// 마스터 키를 키체인으로 옮겨도 · 되돌려도 저장된 비밀번호가 그대로 풀린다. 키체인에 없으면 새 키를 만들지 않는다
    #[test]
    fn master_key_moves_between_file_and_keychain() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let vault = MemVault::default();
        SecretFile::open_with(&dir, &vault).unwrap().set("a", Some("pw")).unwrap();
        move_key(&dir, KeyLocation::Keychain, &vault).unwrap();
        assert_eq!(key_location(&dir), KeyLocation::Keychain);
        assert!(!dir.join("secret.key").exists());
        assert_eq!(SecretFile::open_with(&dir, &vault).unwrap().get("a").unwrap().as_deref(), Some("pw"));
        assert!(SecretFile::open_with(&dir, &MemVault::default()).is_err(), "no key in the keychain: don't start over");
        assert!(!dir.join("secret.key").exists());
        move_key(&dir, KeyLocation::File, &vault).unwrap();
        assert_eq!(key_location(&dir), KeyLocation::File);
        assert!(vault.0.lock().unwrap().is_empty());
        assert_eq!(SecretFile::open_with(&dir, &vault).unwrap().get("a").unwrap().as_deref(), Some("pw"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 예전 형식 (AAD 없음) 은 한 번 읽으면 새 형식으로 바뀐다 (옮겨 붙인 암호문은 다시 풀리지 않는다)
    #[test]
    fn legacy_entries_are_upgraded() {
        let dir = std::env::temp_dir().join(format!("dbtool-secrets-{}", uuid::Uuid::new_v4()));
        let s = SecretFile::open(&dir).unwrap();
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = s.cipher.encrypt(Nonce::from_slice(&nonce), b"old".as_ref()).unwrap();
        let enc = base64::engine::general_purpose::STANDARD.encode([nonce.to_vec(), ct].concat());
        std::fs::write(dir.join("secrets.json"), serde_json::json!({ "a": enc }).to_string()).unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("old"));
        let now: HashMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("secrets.json")).unwrap()).unwrap();
        assert_ne!(now["a"], enc);
        // 다른 이름으로 옮겨 붙이면 풀리지 않는다
        std::fs::write(dir.join("secrets.json"), serde_json::json!({ "b": now["a"] }).to_string()).unwrap();
        assert_eq!(s.get("b").unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
