//! 쿼리 히스토리 (실행한 문장)와 스니펫 (이름 붙여 저장한 쿼리). GUI·TUI 가 같이 쓴다.
//! 히스토리: config/history.jsonl — 한 줄에 하나씩 덧붙인다 (두 앱이 동시에 써도 서로 덮지 않게), 너무 길어지면 최근 것만 남긴다.
//! 비밀번호는 남기지 않는다 (PASSWORD '…', IDENTIFIED BY '…', AUTH …)
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
    /// 밀리초 (Unix)
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub sql: String,
    /// 이 Data Source 에서만 보인다 (없으면 어디서나)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conn_id: Option<String>,
}

/// 기록에 남기지 않을 비밀번호를 가린다 (GUI 예전 redactSecrets 와 같은 규칙)
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

/// 남길 기록 수
pub const MAX_ITEMS: usize = 5000;
/// 한 문장은 이만큼까지만 (큰 스크립트를 통째로 남기지 않게)
const MAX_SQL: usize = 64 * 1024;

pub struct History {
    dir: PathBuf,
    /// 파일의 줄 수 (처음 한 번 세고 덧붙일 때마다 더한다: 매번 파일을 다 읽지 않게. 다른 앱이 쓴 줄은 다음에 셀 때 들어간다)
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
        // 한 번에 덧붙인다 (O_APPEND: 다른 앱이 동시에 써도 줄이 섞이지 않는다)
        opts.open(self.file())?.write_all(line.as_bytes())?;
        // 길어졌으면 최근 것만 남긴다
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

    /// 최근 것부터. conn = 그 Data Source 만, filter = 글이 들어 있는 것 (대소문자 무시). 같은 문장은 한 번만
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

    /// 예전 GUI 기록(localStorage)을 한 번 옮긴다. 이미 있는 id 는 건너뛴다
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

    /// 이름순. conn 이면 그 Data Source 것과 어디서나 쓰는 것
    pub fn snippets(&self, conn: Option<&str>) -> Vec<Snippet> {
        let mut v: Vec<Snippet> = self.all_snippets().into_iter().filter(|s| s.conn_id.is_none() || conn.is_none() || s.conn_id.as_deref() == conn).collect();
        v.sort_by_key(|s| s.name.to_lowercase());
        v
    }

    /// 같은 id 면 바꾼다 (없으면 새 id). 저장한 스니펫
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
        // 비밀번호는 남지 않는다
        h.add("a", None, "create user x password 'pw'").unwrap();
        assert_eq!(h.list(Some("a"), "", 1)[0].sql, "create user x password '***'");
        // 예전 기록 옮기기: 이미 있는 id 는 건너뛴다
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
