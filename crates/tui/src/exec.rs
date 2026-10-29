//! What ⌃Enter runs (DataGrip Settings › Database › Query Execution, same rules as the GUI's src/lib/execPlan.ts)
use qurry_core::sql;
use qurry_core::store::DbKind;

/// When the cursor is inside a statement
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Inside {
    Ask,
    Smallest,
    Whole,
    FromCaret,
}
/// When the cursor is outside a statement (blank line etc.)
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outside {
    Nothing,
    Whole,
    Below,
}
/// When running a selection
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Selection {
    Single,
    Separate,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ExecSettings {
    pub inside: Inside,
    pub outside: Outside,
    pub selection: Selection,
}

/// Like DataGrip, asks what to run when the cursor is inside a statement
impl Default for ExecSettings {
    fn default() -> Self {
        ExecSettings { inside: Inside::Ask, outside: Outside::Nothing, selection: Selection::Separate }
    }
}

impl ExecSettings {
    /// Value written to workspace-tui.json (same names as the GUI's ExecSettings)
    pub fn to_json(self) -> serde_json::Value {
        let inside = match self.inside {
            Inside::Ask => "ask",
            Inside::Smallest => "smallest",
            Inside::Whole => "whole",
            Inside::FromCaret => "fromCaret",
        };
        let outside = match self.outside {
            Outside::Nothing => "nothing",
            Outside::Whole => "whole",
            Outside::Below => "below",
        };
        let selection = match self.selection {
            Selection::Single => "single",
            Selection::Separate => "separate",
        };
        serde_json::json!({ "inside": inside, "outside": outside, "selection": selection })
    }

    /// Unknown values fall back to the default
    pub fn from_json(v: &serde_json::Value) -> Self {
        let d = Self::default();
        let g = |k: &str| v.get(k).and_then(serde_json::Value::as_str).unwrap_or_default();
        ExecSettings {
            inside: match g("inside") {
                "ask" => Inside::Ask,
                "smallest" => Inside::Smallest,
                "whole" => Inside::Whole,
                "fromCaret" => Inside::FromCaret,
                _ => d.inside,
            },
            outside: match g("outside") {
                "nothing" => Outside::Nothing,
                "whole" => Outside::Whole,
                "below" => Outside::Below,
                _ => d.outside,
            },
            selection: match g("selection") {
                "single" => Selection::Single,
                "separate" => Selection::Separate,
                _ => d.selection,
            },
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Plan {
    Run(Vec<String>),
    /// (display name, statements to run)
    Ask(Vec<(String, Vec<String>)>),
    Nothing,
}

/// Keywords that can start a statement (non-SQL lines before such a line are treated as titles·notes and dropped)
const SQL_START: &[&str] = &[
    "select", "with", "insert", "update", "delete", "merge", "upsert", "begin", "start", "commit", "rollback", "end", "abort", "savepoint", "release", "grant", "revoke", "create", "alter", "drop",
    "truncate", "comment", "set", "reset", "show", "explain", "analyze", "vacuum", "values", "table", "copy", "call", "do", "lock", "declare", "fetch", "close", "prepare", "execute",
    "deallocate", "listen", "notify", "unlisten", "refresh", "reindex", "cluster", "discard", "checkpoint", "use", "describe", "desc", "replace", "load", "handler", "optimize", "kill",
    "flush", "rename", "import",
];

fn first_word(s: &str) -> String {
    s.trim_start().chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase()
}

fn starts_sql(line: &str) -> bool {
    line.trim_start().starts_with('(') || SQL_START.contains(&first_word(line).as_str())
}

/// Start position after dropping leading non-SQL lines (numbered titles etc.). Unchanged if no line starts with a keyword
fn skip_notes(text: &str, start: usize, end: usize) -> usize {
    let mut pos = start;
    let mut cut = start;
    while pos < end {
        let line_end = text[pos..end].find('\n').map(|i| pos + i).unwrap_or(end);
        let line = &text[pos..line_end];
        if starts_sql(line) {
            return cut;
        }
        let t = line.trim_start();
        if !(t.is_empty() || t.starts_with("--") || t.starts_with("/*") || t.starts_with('#')) {
            cut = (line_end + 1).min(end);
        }
        pos = line_end + 1;
    }
    start
}

fn is_tx_begin(s: &str) -> bool {
    let w = first_word(s);
    w == "begin" || (w == "start" && s.to_ascii_lowercase().split_whitespace().nth(1) == Some("transaction"))
}
fn is_tx_end(s: &str) -> bool {
    matches!(first_word(s).as_str(), "commit" | "rollback" | "end" | "abort")
}

/// The nearest BEGIN … COMMIT/ROLLBACK enclosing the cursor statement (idx) (statement index range)
fn tx_around(all: &[String], idx: usize) -> Option<(usize, usize)> {
    let mut b = idx as isize;
    while b >= 0 && !is_tx_begin(&all[b as usize]) {
        if (b as usize) < idx && is_tx_end(&all[b as usize]) {
            return None;
        }
        b -= 1;
    }
    if b < 0 {
        return None;
    }
    let b = b as usize;
    let mut e = if idx > b && is_tx_end(&all[idx]) { idx } else { idx.max(b + 1) };
    while e < all.len() && !is_tx_end(&all[e]) {
        e += 1;
    }
    (e < all.len()).then_some((b, e))
}

/// Without a selection, pick the statement to run from the cursor position (offset, bytes)
pub fn plan(text: &str, offset: usize, kind: DbKind, s: ExecSettings) -> Plan {
    // (start for cursor matching — including title lines, end, statement to run — without title lines)
    let segs: Vec<(usize, usize, String)> = sql::split_script(text, kind)
        .into_iter()
        .map(|(a, b)| {
            let raw = &text[a..b];
            let from = a + (raw.len() - raw.trim_start().len());
            let end = a + raw.trim_end().len();
            let at = if kind == DbKind::Redis { from } else { skip_notes(text, from, end) };
            (from, end, text[at..end].trim().to_string())
        })
        .collect();
    let all: Vec<String> = segs.iter().map(|x| x.2.clone()).collect();
    if all.is_empty() {
        return Plan::Nothing;
    }
    let Some(idx) = segs.iter().position(|x| offset >= x.0 && offset <= x.1 + 1) else {
        return match s.outside {
            Outside::Whole => Plan::Run(all),
            Outside::Below => Plan::Run(segs.iter().filter(|x| x.0 >= offset).map(|x| x.2.clone()).collect()),
            // If the previous statement ends on the same line, that statement (cursor right after the semicolon)
            Outside::Nothing => match segs.iter().rev().find(|x| x.1 <= offset && !text[x.1..offset].contains('\n')) {
                Some(x) => Plan::Run(vec![x.2.clone()]),
                None => Plan::Nothing,
            },
        };
    };
    match s.inside {
        Inside::Smallest => Plan::Run(vec![all[idx].clone()]),
        Inside::Whole => Plan::Run(all),
        Inside::FromCaret => Plan::Run(all[idx..].to_vec()),
        Inside::Ask => {
            let label: String = all[idx].split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
            let mut opts = vec![(label, vec![all[idx].clone()])];
            if let Some((b, e)) = tx_around(&all, idx).filter(|(b, e)| e - b + 1 < all.len()) {
                let w = |x: &str| first_word(x).to_uppercase();
                opts.push((format!("Transaction ({} … {}, {} statements)", w(&all[b]), w(&all[e]), e - b + 1), all[b..=e].to_vec()));
            }
            if all.len() > 1 {
                opts.push((format!("All statements ({})", all.len()), all.clone()));
            }
            if idx > 0 && idx < all.len() - 1 {
                opts.push(("From caret to end".into(), all[idx..].to_vec()));
            }
            if opts.len() == 1 { Plan::Run(opts.remove(0).1) } else { Plan::Ask(opts) }
        }
    }
}

/// When running a selection
pub fn plan_selection(sel: &str, kind: DbKind, s: ExecSettings) -> Plan {
    let v: Vec<String> = match s.selection {
        Selection::Single => vec![sel.trim().to_string()],
        Selection::Separate => sql::split_script(sel, kind).into_iter().map(|(a, b)| sel[a..b].trim().to_string()).collect(),
    };
    let v: Vec<String> = v.into_iter().filter(|x| !x.is_empty()).collect();
    if v.is_empty() { Plan::Nothing } else { Plan::Run(v) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQL: &str = "BEGIN;\nGRANT a TO x;\n\nGRANT b TO x;\nCOMMIT;";
    fn at(needle: &str) -> usize {
        SQL.find(needle).unwrap() + 1
    }
    fn p(offset: usize, s: ExecSettings) -> Plan {
        plan(SQL, offset, DbKind::Postgres, s)
    }
    fn run(v: &[&str]) -> Plan {
        Plan::Run(v.iter().map(|s| s.to_string()).collect())
    }
    const ALL: [&str; 4] = ["BEGIN", "GRANT a TO x", "GRANT b TO x", "COMMIT"];

    #[test]
    fn default_asks_inside_statement() {
        let Plan::Ask(opts) = p(at("GRANT b"), ExecSettings::default()) else { panic!("should ask") };
        let labels: Vec<_> = opts.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(labels, ["GRANT b TO x", "All statements (4)", "From caret to end"]);
        assert_eq!(opts[1].1, ALL);
        assert_eq!(opts[2].1, ["GRANT b TO x", "COMMIT"]);
        // First statement: 'from cursor to end' equals the whole, so drop it
        let Plan::Ask(opts) = p(at("BEGIN"), ExecSettings::default()) else { panic!("should ask") };
        assert_eq!(opts.len(), 2);
        // Don't ask if there's only one statement
        assert_eq!(plan("select 1", 3, DbKind::Postgres, ExecSettings::default()), run(&["select 1"]));
    }

    /// Statements on separate lines without `;`: only the caret line's statement (not joined with the previous line)
    #[test]
    fn statements_without_semicolons() {
        let t = "select * from logs.user_activity_log where user_idx = 1\nselect * from logs.user_activity_log where user_idx = 3\nselect * from public.users";
        let s = ExecSettings { inside: Inside::Smallest, ..Default::default() };
        let at = t.find("public.users").unwrap();
        assert_eq!(plan(t, at, DbKind::Postgres, s), run(&["select * from public.users"]));
        let Plan::Ask(opts) = plan(t, at, DbKind::Postgres, ExecSettings::default()) else { panic!("should ask") };
        assert_eq!(opts[0].0, "select * from public.users");
    }

    #[test]
    fn inside_modes() {
        let s = |inside| ExecSettings { inside, ..Default::default() };
        assert_eq!(p(at("GRANT b"), s(Inside::Smallest)), run(&["GRANT b TO x"]));
        assert_eq!(p(at("GRANT b"), s(Inside::Whole)), run(&ALL));
        assert_eq!(p(at("GRANT b"), s(Inside::FromCaret)), run(&["GRANT b TO x", "COMMIT"]));
        // A cursor right after a semicolon belongs to that statement
        let end = SQL.find("GRANT a TO x;").unwrap() + "GRANT a TO x;".len();
        assert_eq!(p(end, s(Inside::Smallest)), run(&["GRANT a TO x"]));
    }

    #[test]
    fn outside_modes() {
        let blank = SQL.find("\n\n").unwrap() + 1;
        let s = |outside| ExecSettings { outside, ..Default::default() };
        assert_eq!(p(blank, s(Outside::Nothing)), Plan::Nothing);
        assert_eq!(p(blank, s(Outside::Whole)), run(&ALL));
        assert_eq!(p(blank, s(Outside::Below)), run(&["GRANT b TO x", "COMMIT"]));
    }

    #[test]
    fn settings_round_trip() {
        let e = ExecSettings { inside: Inside::FromCaret, outside: Outside::Below, selection: Selection::Single };
        assert_eq!(ExecSettings::from_json(&e.to_json()), e);
        assert_eq!(ExecSettings::from_json(&serde_json::json!({ "inside": "bogus" })), ExecSettings::default());
    }

    #[test]
    fn selection_modes() {
        let sel = "GRANT a TO x;\n\nGRANT b TO x;";
        assert_eq!(plan_selection(sel, DbKind::Postgres, ExecSettings::default()), run(&["GRANT a TO x", "GRANT b TO x"]));
        let single = ExecSettings { selection: Selection::Single, ..Default::default() };
        assert_eq!(plan_selection(sel, DbKind::Postgres, single), run(&[sel]));
    }
    #[test]
    fn skips_note_lines_before_statement() {
        let sql = "1. 공격 페이로드 탐지 (SSRF)\nSELECT 1\nFROM t;\n\n2. 거부 에러코드\n-- 설명\nSELECT 2;";
        let s = ExecSettings { inside: Inside::Smallest, ..Default::default() };
        assert_eq!(plan(sql, 3, DbKind::Postgres, s), run(&["SELECT 1\nFROM t"]));
        assert_eq!(plan(sql, sql.find("2. 거부").unwrap() + 1, DbKind::Postgres, s), run(&["-- 설명\nSELECT 2"]));
        assert_eq!(plan("foo bar", 1, DbKind::Postgres, s), run(&["foo bar"]));
    }

    #[test]
    fn offers_transaction_around_caret() {
        let sql = "BEGIN;\nGRANT a TO x;\nGRANT b TO x;\nCOMMIT;\n\nSELECT 1;";
        let Plan::Ask(opts) = plan(sql, sql.find("GRANT b").unwrap() + 1, DbKind::Postgres, ExecSettings::default()) else { panic!("should ask") };
        let labels: Vec<_> = opts.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(labels, ["GRANT b TO x", "Transaction (BEGIN … COMMIT, 4 statements)", "All statements (5)", "From caret to end"]);
        assert_eq!(opts[1].1, ["BEGIN", "GRANT a TO x", "GRANT b TO x", "COMMIT"]);
        let Plan::Ask(opts) = plan(sql, sql.find("SELECT 1").unwrap() + 1, DbKind::Postgres, ExecSettings::default()) else { panic!("should ask") };
        assert_eq!(opts.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), ["SELECT 1", "All statements (5)"]);
    }
}

