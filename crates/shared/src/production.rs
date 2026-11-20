//! The Production confirmation (a data source with "Production" on): which statements of a run it asks about and what it says of
//! each. One rule for the GUI (through WASM) and the TUI; each app keeps its own dialog title and layout around these lines.
//! UI decisions only, not a security boundary: read-only Data Sources and MCP are enforced in the backend (qurry_core::guard).

use crate::DbKind;
use crate::format::thousands;
use crate::guard;
use serde::Serialize;

/// The statements the Production confirmation lists: those that change data (`guard::is_read` is false, so a write keyword only
/// inside a string or a comment doesn't count). Empty for Explain: it asks its own question
pub fn asked_statements(stmts: &[String], kind: DbKind, explain: bool) -> Vec<&str> {
    if explain {
        return vec![];
    }
    stmts.iter().map(String::as_str).filter(|s| !guard::is_read(s, kind)).collect()
}

/// How prominent a statement's line is: more rows changed = more prominent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    None,
    Warn,
    Danger,
}

/// The tone of a row count: 0 rows none, 1–99 warn, 100+ or not counted danger
pub fn tone(rows: Option<u64>) -> Tone {
    match rows {
        Some(0) => Tone::None,
        Some(n) if n < 100 => Tone::Warn,
        _ => Tone::Danger,
    }
}

/// What the confirmation says of one statement it lists, from its row count (`Ok(None)`: the statement can't be counted;
/// `Err`: counting failed): "1 row", "12,408 rows", "schema change" (DDL), "can't count", "can't count: <first line of the error>"
pub fn impact_line(sql: &str, kind: DbKind, count: Result<Option<u64>, String>) -> (String, Tone) {
    match count {
        Ok(Some(n)) => (format!("{} row{}", thousands(n), if n == 1 { "" } else { "s" }), tone(Some(n))),
        Ok(None) if guard::is_ddl(sql, kind) => ("schema change".into(), Tone::Danger),
        Ok(None) => ("can't count".into(), Tone::Danger),
        Err(e) => (format!("can't count: {}", e.lines().next().unwrap_or("")), Tone::Danger),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DbKind::{Mysql, Postgres};

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn asks_about_writes_only() {
        let stmts = v(&["select 1", "update t set a = 1", "delete from t where id = 2", "show tables"]);
        assert_eq!(asked_statements(&stmts, Postgres, false), ["update t set a = 1", "delete from t where id = 2"]);
        assert_eq!(asked_statements(&v(&["select 1", "create table x (a int)"]), Mysql, false), ["create table x (a int)"]);
    }

    #[test]
    fn explain_asks_nothing() {
        assert!(asked_statements(&v(&["delete from t", "drop table t"]), Postgres, true).is_empty());
    }

    #[test]
    fn write_word_in_string_or_comment_is_not_asked() {
        let stmts = v(&["select 'delete' as action", "select 1 -- then delete from t", "select 1 /* update t */"]);
        assert!(asked_statements(&stmts, Postgres, false).is_empty());
        assert!(asked_statements(&v(&["select 1 # then delete from t", "# drop table t\nselect 1"]), Mysql, false).is_empty());
        // A quoted name is still a write word to the check: it asks
        assert_eq!(asked_statements(&v(&["select \"update\" from t"]), Postgres, false), ["select \"update\" from t"]);
    }

    #[test]
    fn counted_rows_and_tones() {
        let line = |n| impact_line("update t set a = 1", Postgres, Ok(Some(n)));
        assert_eq!(line(0), ("0 rows".into(), Tone::None));
        assert_eq!(line(1), ("1 row".into(), Tone::Warn));
        assert_eq!(line(99), ("99 rows".into(), Tone::Warn));
        assert_eq!(line(100), ("100 rows".into(), Tone::Danger));
        assert_eq!(line(12408), ("12,408 rows".into(), Tone::Danger));
    }

    #[test]
    fn uncounted_ddl_and_errors() {
        assert_eq!(impact_line("create table x (a int)", Postgres, Ok(None)), ("schema change".into(), Tone::Danger));
        assert_eq!(impact_line("-- note\nALTER TABLE x ADD b int", Mysql, Ok(None)), ("schema change".into(), Tone::Danger));
        assert_eq!(impact_line("insert into t values (1)", Postgres, Ok(None)), ("can't count".into(), Tone::Danger));
        assert_eq!(
            impact_line("delete from t", Postgres, Err("connection refused\nis the server running?".into())),
            ("can't count: connection refused".into(), Tone::Danger)
        );
        assert_eq!(impact_line("delete from t", Postgres, Err(String::new())), ("can't count: ".into(), Tone::Danger));
    }

    #[test]
    fn tone_thresholds() {
        assert_eq!([tone(Some(0)), tone(Some(1)), tone(Some(99)), tone(Some(100)), tone(None)], [Tone::None, Tone::Warn, Tone::Warn, Tone::Danger, Tone::Danger]);
    }
}
