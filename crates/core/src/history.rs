//! Query history (statements run) and snippets (named saved queries). Shared by GUI and TUI.
//! History: config/history.jsonl — appended one per line (so the two apps don't overwrite each other when writing at once); when it gets too long, only recent entries are kept.
//! Passwords are not recorded (PASSWORD '…', IDENTIFIED BY '…', AUTH …)
use crate::error::AppResult;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub conn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub sql: String,
    /// Milliseconds (Unix)
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub sql: String,
    /// Shown only in this Data Source (none = everywhere)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conn_id: Option<String>,
}

/// Mask passwords that shouldn't be recorded (same rules as the old GUI redactSecrets)
pub fn redact(sql: &str) -> String {
    use std::sync::LazyLock;
    static RULES: LazyLock<Vec<(regex::Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r#"(?i)(\bpassword\s*(?:=\s*)?)('(?:[^']|'')*'|"[^"]*")"#, "${1}'***'"),
            (r#"(?i)(\bidentified\s+(?:with\s+\S+\s+)?by\s+)('(?:[^'\\]|\\.|'')*'|"[^"]*")"#, "${1}'***'"),
            (r"(?im)^(\s*auth\s+)(?:\S+\s+)?\S+\s*$", "${1}***"),
            (r"(?i)(\b(?:requirepass|masterauth)\s+)\S+", "${1}***"),
        ]
        .into_iter()
        .map(|(p, r)| (regex::Regex::new(p).unwrap(), r))
        .collect()
    });
    let mut out = sql.to_string();
    for (re, with) in RULES.iter() {
        out = re.replace_all(&out, *with).into_owned();
    }
    out
}

/// Number of entries to keep
pub const MAX_ITEMS: usize = 5000;
/// A single statement is kept only up to this length (so large scripts aren't stored whole)
const MAX_SQL: usize = 64 * 1024;

pub struct History {
    dir: PathBuf,
    /// Line count of the file (counted once, then incremented on each append: avoids reading the whole file each time. Lines written by the other app are included at the next count)
    lines: std::sync::Mutex<Option<usize>>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl History {
    pub fn new(dir: &Path) -> Self {
        History { dir: dir.to_path_buf(), lines: Default::default() }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("history.jsonl")
    }

    fn snippet_file(&self) -> PathBuf {
        self.dir.join("snippets.json")
    }

    fn read(&self) -> Vec<HistoryItem> {
        let Ok(s) = std::fs::read_to_string(self.file()) else { return vec![] };
        s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn rewrite(&self, items: &[HistoryItem]) -> AppResult<()> {
        if let Ok(mut n) = self.lines.try_lock() {
            *n = Some(items.len());
        }
        let mut out = String::new();
        for i in items {
            out.push_str(&serde_json::to_string(i)?);
            out.push('\n');
        }
        crate::secrets::write_private(&self.file(), out.as_bytes())
    }

    fn append(&self, items: &[HistoryItem]) -> AppResult<()> {
        use std::io::Write;
        let mut line = String::new();
        for i in items {
            line.push_str(&serde_json::to_string(i)?);
            line.push('\n');
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        // Append in one write (O_APPEND: lines don't interleave even if the other app writes at the same time)
        opts.open(self.file())?.write_all(line.as_bytes())?;
        // If it got long, keep only recent entries
        let mut n = self.lines.lock().unwrap();
        let count = match *n {
            Some(c) => c + items.len(),
            None => std::fs::read(self.file()).map(|b| b.iter().filter(|&&c| c == b'\n').count()).unwrap_or(0),
        };
        *n = Some(count);
        if count > MAX_ITEMS + MAX_ITEMS / 10 {
            let all = self.read();
            let keep = &all[all.len().saturating_sub(MAX_ITEMS)..];
            self.rewrite(keep)?;
            *n = Some(keep.len());
        }
        Ok(())
    }

    pub fn add(&self, conn_id: &str, database: Option<&str>, sql: &str) -> AppResult<()> {
        let mut sql = redact(sql.trim());
        if sql.is_empty() {
            return Ok(());
        }
        if sql.len() > MAX_SQL {
            let mut cut = MAX_SQL;
            while !sql.is_char_boundary(cut) {
                cut -= 1;
            }
            sql.truncate(cut);
        }
        let item = HistoryItem { id: uuid::Uuid::new_v4().to_string(), conn_id: conn_id.into(), database: database.map(String::from), sql, ts: now_ms() };
        self.append(&[item])
    }

    /// Most recent first. conn = only that Data Source, filter = entries containing the text (case-insensitive). Identical statements only once
    pub fn list(&self, conn: Option<&str>, filter: &str, limit: usize) -> Vec<HistoryItem> {
        let f = filter.to_lowercase();
        let mut all = self.read();
        all.sort_by_key(|i| i.ts);
        let mut seen = std::collections::HashSet::new();
        all.into_iter()
            .rev()
            .filter(|i| conn.is_none_or(|c| i.conn_id == c))
            .filter(|i| f.is_empty() || i.sql.to_lowercase().contains(&f))
            .filter(|i| seen.insert((i.conn_id.clone(), i.sql.clone())))
            .take(limit.min(MAX_ITEMS))
            .collect()
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let all = self.read();
        if all.iter().any(|i| i.id == id) {
            self.rewrite(&all.into_iter().filter(|i| i.id != id).collect::<Vec<_>>())?;
        }
        Ok(())
    }

    /// Migrate the old GUI history (localStorage) once. Existing ids are skipped
    pub fn import(&self, items: Vec<HistoryItem>) -> AppResult<usize> {
        let mut have: std::collections::HashSet<String> = self.read().into_iter().map(|i| i.id).collect();
        let new: Vec<HistoryItem> = items
            .into_iter()
            .filter(|i| have.insert(i.id.clone()))
            .map(|i| HistoryItem { sql: redact(&i.sql), ..i })
            .collect();
        if !new.is_empty() {
            self.append(&new)?;
        }
        Ok(new.len())
    }

    fn all_snippets(&self) -> Vec<Snippet> {
        std::fs::read_to_string(self.snippet_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// By name. With conn, that Data Source's snippets plus the ones usable everywhere
    pub fn snippets(&self, conn: Option<&str>) -> Vec<Snippet> {
        let mut v: Vec<Snippet> = self.all_snippets().into_iter().filter(|s| s.conn_id.is_none() || conn.is_none() || s.conn_id.as_deref() == conn).collect();
        v.sort_by_key(|s| s.name.to_lowercase());
        v
    }

    /// Replace if the id matches (new id if none). The saved snippet
    pub fn save_snippet(&self, mut s: Snippet) -> AppResult<Snippet> {
        if s.id.is_empty() {
            s.id = uuid::Uuid::new_v4().to_string();
        }
        let mut all = self.all_snippets();
        match all.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s.clone(),
            None => all.push(s.clone()),
        }
        crate::secrets::write_private(&self.snippet_file(), serde_json::to_string_pretty(&all)?.as_bytes())?;
        Ok(s)
    }

    pub fn delete_snippet(&self, id: &str) -> AppResult<()> {
        let all: Vec<Snippet> = self.all_snippets().into_iter().filter(|s| s.id != id).collect();
        crate::secrets::write_private(&self.snippet_file(), serde_json::to_string_pretty(&all)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (History, PathBuf) {
        let d = std::env::temp_dir().join(format!("qurry-hist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        (History::new(&d), d)
    }

    #[test]
    fn redacts_passwords() {
        assert_eq!(redact("CREATE USER a PASSWORD 'se''cret'"), "CREATE USER a PASSWORD '***'");
        assert_eq!(redact("alter role a with password = 'x'"), "alter role a with password = '***'");
        assert_eq!(redact("CREATE USER 'a'@'%' IDENTIFIED BY 'p@ss'"), "CREATE USER 'a'@'%' IDENTIFIED BY '***'");
        assert_eq!(redact("AUTH default s3cret"), "AUTH ***");
        assert_eq!(redact("select 1"), "select 1");
    }

    #[test]
    fn history_newest_first_filtered_and_deduplicated() {
        let (h, d) = tmp();
        h.add("a", Some("shop"), "select 1").unwrap();
        h.add("a", None, "select 2").unwrap();
        h.add("b", None, "select 3").unwrap();
        h.add("a", None, "select 1").unwrap();
        let sqls = |v: Vec<HistoryItem>| v.into_iter().map(|i| i.sql).collect::<Vec<_>>();
        assert_eq!(sqls(h.list(Some("a"), "", 10)), ["select 1", "select 2"]);
        assert_eq!(sqls(h.list(None, "", 10)), ["select 1", "select 3", "select 2"]);
        assert_eq!(sqls(h.list(None, "SELECT 3", 10)), ["select 3"]);
        let id = h.list(Some("b"), "", 1)[0].id.clone();
        h.delete(&id).unwrap();
        assert!(h.list(Some("b"), "", 10).is_empty());
        // Passwords are not recorded
        h.add("a", None, "create user x password 'pw'").unwrap();
        assert_eq!(h.list(Some("a"), "", 1)[0].sql, "create user x password '***'");
        // Migrating old history: existing ids are skipped
        let old = HistoryItem { id: "old1".into(), conn_id: "a".into(), database: None, sql: "select 0".into(), ts: 1 };
        assert_eq!(h.import(vec![old.clone(), old]).unwrap(), 1);
        assert_eq!(h.list(Some("a"), "select 0", 5).len(), 1);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn keeps_only_recent_history() {
        let (h, d) = tmp();
        for i in 0..(MAX_ITEMS + 50) {
            h.add("a", None, &format!("select {i}")).unwrap();
        }
        let all = h.list(None, "", usize::MAX);
        assert!(all.len() <= MAX_ITEMS, "{}", all.len());
        assert_eq!(all[0].sql, format!("select {}", MAX_ITEMS + 49));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn snippets_crud_and_scope() {
        let (h, d) = tmp();
        let a = h.save_snippet(Snippet { id: String::new(), name: "suspicious IPs".into(), sql: "select 1".into(), conn_id: Some("prod".into()) }).unwrap();
        assert!(!a.id.is_empty());
        h.save_snippet(Snippet { id: String::new(), name: "anywhere".into(), sql: "select 2".into(), conn_id: None }).unwrap();
        let names = |v: Vec<Snippet>| v.into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names(h.snippets(Some("prod"))), ["anywhere", "suspicious IPs"]);
        assert_eq!(names(h.snippets(Some("dev"))), ["anywhere"]);
        h.save_snippet(Snippet { name: "renamed".into(), ..a.clone() }).unwrap();
        assert_eq!(names(h.snippets(Some("prod"))), ["anywhere", "renamed"]);
        h.delete_snippet(&a.id).unwrap();
        assert_eq!(names(h.snippets(Some("prod"))), ["anywhere"]);
        std::fs::remove_dir_all(d).unwrap();
    }
}
