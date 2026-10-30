//! Statement checks (read-only, risky statements, MCP execute): the code is qurry_shared::guard, re-exported here so every caller
//! keeps using qurry_core::guard. Only impact_count stays in core, since it reads a QueryResult.
//! The GUI's WASM copy of the checks is for UI decisions only; enforcement (read-only Data Sources, MCP) runs these in the backend.
pub use qurry_shared::guard::*;

/// The number in the first cell of the count query's result (drivers such as MySQL may return it as text)
pub fn impact_count(r: &crate::db::QueryResult) -> Option<i64> {
    r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DbKind;

    /// The count comes back as a number (PostgreSQL) or as text (MySQL and some drivers)
    #[test]
    fn impact_count_reads_numbers_and_text() {
        let r = |v: serde_json::Value| crate::db::QueryResult { columns: vec![], rows: vec![vec![v]], rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None };
        assert_eq!(impact_count(&r(12408.into())), Some(12408));
        assert_eq!(impact_count(&r("12408".into())), Some(12408));
        assert_eq!(impact_count(&r(serde_json::Value::Null)), None);
        assert_eq!(impact_count(&crate::db::QueryResult { rows: vec![], ..r(1.into()) }), None);
    }

    /// Core's guard, sql and DbKind are the shared items, not copies: two glob imports of the same name only compile
    /// (instead of failing as ambiguous, E0659) when both paths name the same item
    #[test]
    fn guard_sql_and_kind_are_the_shared_items() {
        // rustc reports the second import of an item as unused: that is the point
        #[allow(unused_imports)]
        mod both {
            pub use crate::guard::*;
            pub use crate::sql::*;
            pub use crate::store::*;
            pub use qurry_shared::guard::*;
            pub use qurry_shared::kind::*;
            pub use qurry_shared::sql::*;
        }
        let read_only_violation: fn(&str, DbKind, bool) -> Option<String> = both::read_only_violation;
        let kind: both::DbKind = DbKind::Postgres;
        assert!(read_only_violation("delete from t", kind, true).is_some());
        assert_eq!(both::split_statements("select 1; select 2", kind).len(), 2);
        assert!(both::unsafe_statement("drop table t", kind).is_some());
    }
}
