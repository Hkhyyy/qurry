//! Object·column list cache for search (like DataGrip's introspection cache).
//! Saved on every read while connected, so Search Everywhere can find unconnected Data Sources too.
//! Holds only names, but it's DB structure, so it's written as 0600. Separate object·column files per target (so concurrent updates don't overwrite each other)
use crate::meta::{CompletionTable, IndexEntry};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedIndex {
    /// "<data source id>::<database>"
    pub target: String,
    pub objects: Vec<IndexEntry>,
    pub columns: Vec<CompletionTable>,
}

fn dir(base: &Path) -> PathBuf {
    base.join("search-cache")
}

/// File name: target id in hex (safe even if the DB name has characters like /)
fn file(base: &Path, target: &str, part: &str) -> PathBuf {
    let hex: String = target.bytes().map(|b| format!("{b:02x}")).collect();
    dir(base).join(format!("{hex}.{part}.json"))
}

fn unhex(s: &str) -> Option<String> {
    let b: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect();
    String::from_utf8(b?).ok()
}

fn save<T: Serialize>(base: &Path, target: &str, part: &str, v: &T) {
    if let Ok(data) = serde_json::to_vec(v) {
        if let Err(e) = crate::secrets::write_private(&file(base, target, part), &data) {
            crate::log!("[qurry] search cache write failed: {e}");
        }
    }
}

pub fn save_objects(base: &Path, target: &str, v: &[IndexEntry]) {
    save(base, target, "objects", &v);
}

pub fn save_columns(base: &Path, target: &str, v: &[CompletionTable]) {
    save(base, target, "columns", &v);
}

/// Whether the object list was saved within age
pub fn is_fresh(base: &Path, target: &str, age: std::time::Duration) -> bool {
    std::fs::metadata(file(base, target, "objects")).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| e < age)
}

fn read<T: for<'de> Deserialize<'de>>(p: &Path) -> Vec<T> {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// All saved targets (keep: whether the Data Source still exists)
pub fn load_all(base: &Path, keep: impl Fn(&str) -> bool) -> Vec<CachedIndex> {
    let Ok(rd) = std::fs::read_dir(dir(base)) else { return vec![] };
    let mut out = vec![];
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(hex) = name.strip_suffix(".objects.json") else { continue };
        let Some(target) = unhex(hex) else { continue };
        if !keep(crate::app::split_target(&target).0) {
            continue;
        }
        let columns = read(&file(base, &target, "columns"));
        out.push(CachedIndex { objects: read(&e.path()), columns, target });
    }
    out
}

/// Deleting a Data Source deletes its cache too
pub fn remove_conn(base: &Path, conn: &str) {
    let Ok(rd) = std::fs::read_dir(dir(base)) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let hex = name.split('.').next().unwrap_or_default();
        if unhex(hex).is_some_and(|t| crate::app::split_target(&t).0 == conn) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_remove() {
        let base = std::env::temp_dir().join(format!("dbtool-sc-{}", std::process::id()));
        let o = |n: &str| IndexEntry { schema: "public".into(), name: n.into(), kind: "table".into() };
        save_objects(&base, "c1::app/db", &[o("payment")]);
        save_columns(&base, "c1::app/db", &[CompletionTable { schema: "public".into(), name: "payment".into(), columns: vec![("amount".into(), "int".into())] }]);
        save_objects(&base, "c2::x", &[o("other")]);
        let all = load_all(&base, |c| c == "c1");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].target, "c1::app/db");
        assert_eq!(all[0].objects[0].name, "payment");
        assert_eq!(all[0].columns[0].columns[0].0, "amount");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(file(&base, "c1::app/db", "objects")).unwrap().permissions().mode();
            assert_eq!(m & 0o777, 0o600);
        }
        remove_conn(&base, "c1");
        assert!(load_all(&base, |_| true).iter().all(|c| c.target == "c2::x"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
