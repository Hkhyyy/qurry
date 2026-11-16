use super::*;

/// Bright color used for highlighting, like the active tab
pub fn type_color(t: &str) -> Color {
    match t {
        "string" => R_STRING,
        "hash" => R_HASH,
        "list" => R_LIST,
        "set" => R_SET,
        "zset" => R_ZSET,
        "stream" => R_STREAM,
        _ => Color::Gray,
    }
}

/// Output in the same form as redis-cli
pub fn format_reply(v: &Value) -> String {
    fn go(v: &Value, indent: usize) -> String {
        match v {
            Value::Null => "(nil)".into(),
            Value::Number(n) => format!("(integer) {n}"),
            Value::Bool(b) => format!("({b})"),
            Value::String(s) => format!("{s:?}"),
            Value::Object(o) => o.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            Value::Array(a) if a.is_empty() => "(empty array)".into(),
            Value::Array(a) => {
                let w = a.len().to_string().len();
                a.iter()
                    .enumerate()
                    .map(|(i, x)| {
                        let head = format!("{:>w$}) ", i + 1);
                        let body = go(x, indent + head.len());
                        format!("{}{head}{body}", if i == 0 { String::new() } else { " ".repeat(indent) })
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }
    go(v, 0)
}

/// The Redis command line's reply shown by type (qurry_shared::redis_cmd::reply_table, as the GUI's Terminal): a hash as an aligned
/// field / value table, a list numbered, set members one per line; the rest as redis-cli prints it
pub fn format_typed(line: &str, v: &Value) -> String {
    let Some(t) = qurry_shared::redis_cmd::reply_table(line, v) else { return format_reply(v) };
    let w = t.rows.iter().map(|r| r[0].chars().count()).chain([t.heads[0].chars().count()]).max().unwrap_or(0);
    let row = |cells: Vec<&str>| match cells[..] {
        [a, b] => format!("{a:<w$}  {b}"),
        _ => cells.join("  "),
    };
    std::iter::once(row(t.heads.clone())).chain(t.rows.iter().map(|r| row(r.iter().map(String::as_str).collect()))).collect::<Vec<_>>().join("\n")
}
