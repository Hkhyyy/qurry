use qurry_core::db::QueryResult;
use qurry_core::dba;
use qurry_core::sql;
use qurry_core::store::DbKind;
use serde_json::Value;

/// Statement text in a report (dba::QUERY_TEXT_COLUMNS) with its literals replaced by ?, and server messages (dba::ERROR_TEXT_COLUMNS)
/// masked from their first quote to their last: sessions, slow statements and replication errors can hold row values.
/// Names match in any case (SHOW REPLICA STATUS names its columns Last_SQL_Error …)
pub(crate) fn mask_query_text(r: &mut QueryResult, kind: DbKind) {
    let of = |list: &[&str]| -> Vec<usize> { r.columns.iter().enumerate().filter(|(_, c)| list.iter().any(|q| q.eq_ignore_ascii_case(&c.name))).map(|(i, _)| i).collect() };
    let (query, error) = (of(dba::QUERY_TEXT_COLUMNS), of(dba::ERROR_TEXT_COLUMNS));
    for row in &mut r.rows {
        for &i in &query {
            if let Some(Value::String(s)) = row.get_mut(i) {
                *s = sql::mask_literals(s, kind);
            }
        }
        for &i in &error {
            if let Some(Value::String(s)) = row.get_mut(i) {
                *s = mask_quoted(s);
            }
        }
    }
}

/// Everything from the first quote to the last as one ?: MySQL quotes values inside a quoted message without escaping
/// ('Duplicate entry 'x' for key 'k''), so no quote can be trusted to end a value. The words around it stay
fn mask_quoted(s: &str) -> String {
    let is_quote = |c: char| c == '\'' || c == '"';
    match (s.find(is_quote), s.rfind(is_quote)) {
        // A lone quote opens a value that runs to the end
        (Some(a), Some(b)) => format!("{}?{}", &s[..a], if a == b { "" } else { &s[b + 1..] }),
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_query_text_is_masked_by_column() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "TEXT".into() };
        let mut r = QueryResult {
            columns: vec![col("user"), col("blocking_query"), col("query")],
            rows: vec![vec![json!("app'1"), json!("update t set x = 'secret' where id = 42"), Value::Null], vec![json!("b"), json!("select $1"), json!("select E'a\\'b', 7")]],
            rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![],
        };
        mask_query_text(&mut r, DbKind::Postgres);
        assert_eq!(r.rows[0], vec![json!("app'1"), json!("update t set x = ? where id = ?"), Value::Null]);
        assert_eq!(r.rows[1], vec![json!("b"), json!("select $1"), json!("select ?, ?")]);
    }

    /// SHOW REPLICA STATUS: its columns come from the server in its own case, and the last errors quote the failed statement
    #[test]
    fn replica_status_errors_are_masked() {
        let col = |n: &str| qurry_core::db::ColumnInfo { name: n.into(), type_name: "TEXT".into() };
        let sql_error = "Error 'Duplicate entry 'kim@example.com' for key 'users.email'' on query. Default database: 'shop'. Query: 'INSERT INTO users (email) VALUES ('kim@example.com')'";
        let worker = "Worker 1 failed executing transaction 'ANONYMOUS'; Could not execute Write_rows event on table shop.users; Duplicate entry '4242' for key 'users.PRIMARY', Error_code: 1062";
        let mut r = QueryResult {
            columns: vec![col("Replica_IO_Running"), col("Last_Errno"), col("Last_Error"), col("Last_IO_Error"), col("Last_SQL_Error")],
            rows: vec![vec![json!("Yes"), json!(1062), json!(sql_error), json!(""), json!(worker)]],
            rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false, warning: None, editable: None, notices: vec![], cut: vec![],
        };
        mask_query_text(&mut r, DbKind::Mysql);
        assert_eq!(r.rows[0], vec![json!("Yes"), json!(1062), json!("Error ?"), json!(""), json!("Worker 1 failed executing transaction ?, Error_code: 1062")]);
        // One quote: masked to the end
        assert_eq!(mask_quoted("Access denied for user 'repl@10.0.0.6"), "Access denied for user ?");
        assert_eq!(mask_quoted("error reconnecting to source"), "error reconnecting to source");
    }
}
