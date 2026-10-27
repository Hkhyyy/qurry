//! REST 로 붙는 드라이버(벡터 DB)들이 같이 쓰는 것: 콘솔 글 "METHOD /path\n{json}" 읽기, JSON 결과를 표로 펼치기
use crate::{Column, QueryOut};
use serde_json::{Map, Value};

/// 콘솔 글: 첫 줄 "GET /collections" · "POST /x/search", 그 뒤는 본문 (JSON5: 따옴표 없는 키·끝 쉼표 허용)
pub fn parse_request(text: &str) -> Result<(String, String, Option<Value>), String> {
    let t = text.trim().trim_end_matches(';').trim();
    let (first, rest) = t.split_once('\n').unwrap_or((t, ""));
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return Err("expected METHOD /path (e.g. GET /collections) and an optional JSON body on the next lines".into());
    }
    let path = parts.next().ok_or("missing path")?.to_string();
    let path = if path.starts_with('/') { path } else { format!("/{path}") };
    // 같은 줄에 본문이 붙어 있어도 된다
    let inline: String = parts.collect::<Vec<_>>().join(" ");
    let body_text = format!("{inline}\n{rest}");
    let body = if body_text.trim().is_empty() { None } else { Some(json5::from_str::<Value>(body_text.trim()).map_err(|e| format!("can't read the JSON body: {e}"))?) };
    Ok((method, path, body))
}

/// HTTP 로 JSON 을 주고받는 곳: 기준 주소 + 붙일 헤더 (인증)
pub struct Http {
    pub base: String,
    pub headers: Vec<(String, String)>,
    agent: ureq::Agent,
}

impl Http {
    /// host 가 http(s):// 로 시작하면 그대로, 아니면 host:port (ssl 옵션이면 https)
    pub fn new(p: &crate::ConnectParams, default_port: u16) -> Http {
        let tls = p.options.get("ssl").is_some_and(|s| !s.is_null() && s != false);
        let base = if p.host.starts_with("http://") || p.host.starts_with("https://") { p.host.trim_end_matches('/').to_string() } else { format!("{}://{}:{}", if tls { "https" } else { "http" }, p.host, if p.port == 0 { default_port } else { p.port }) };
        let agent = ureq::Agent::config_builder().timeout_connect(Some(std::time::Duration::from_secs(10))).timeout_global(Some(std::time::Duration::from_secs(300))).http_status_as_error(false).build().into();
        Http { base, headers: vec![], agent }
    }

    /// (상태 코드, JSON 본문 — JSON 이 아니면 글 그대로)
    pub fn send(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value), String> {
        let url = format!("{}{path}", self.base);
        let mut res = match method {
            "GET" | "DELETE" => {
                let mut r = if method == "GET" { self.agent.get(&url) } else { self.agent.delete(&url) };
                for (k, v) in &self.headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            m => {
                let mut r = match m {
                    "PUT" => self.agent.put(&url),
                    "PATCH" => self.agent.patch(&url),
                    _ => self.agent.post(&url),
                };
                for (k, v) in &self.headers {
                    r = r.header(k, v);
                }
                r.send_json(body.cloned().unwrap_or(Value::Object(Map::new())))
            }
        }
        .map_err(|e| format!("{}: {e}", self.base))?;
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().unwrap_or_default();
        Ok((status, serde_json::from_str(&text).unwrap_or(Value::String(text))))
    }
}

/// 긴 벡터(숫자 배열)는 앞부분만: [0.12, 0.5, … (1536)]
fn show(v: &Value) -> Value {
    match v {
        Value::Array(a) if a.len() > 8 && a.iter().all(Value::is_number) => {
            let head: Vec<String> = a.iter().take(4).map(|x| x.to_string()).collect();
            Value::String(format!("[{}, … ({})]", head.join(", "), a.len()))
        }
        _ => v.clone(),
    }
}

/// 객체 하나를 열로: payload / metadata 같은 안쪽 객체는 한 단계 펼친다 (payload.city)
fn flatten(o: &Map<String, Value>, nest: &[&str]) -> Vec<(String, Value)> {
    let mut out = vec![];
    for (k, v) in o {
        match v {
            Value::Object(inner) if nest.contains(&k.as_str()) => {
                for (ik, iv) in inner {
                    out.push((format!("{k}.{ik}"), show(iv)));
                }
            }
            _ => out.push((k.clone(), show(v))),
        }
    }
    out
}

/// JSON 결과 → 표. 배열이면 행마다, 객체면 한 행. nest = 펼칠 안쪽 객체 이름
pub fn table(v: &Value, nest: &[&str], limit: usize) -> QueryOut {
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let mut cols: Vec<String> = vec![];
    let mut rows_kv: Vec<Vec<(String, Value)>> = vec![];
    for it in items.iter().take(limit) {
        let kv = match it {
            Value::Object(o) => flatten(o, nest),
            other => vec![("value".to_string(), show(other))],
        };
        for (k, _) in &kv {
            if !cols.contains(k) {
                cols.push(k.clone());
            }
        }
        rows_kv.push(kv);
    }
    let rows = rows_kv.into_iter().map(|kv| cols.iter().map(|c| kv.iter().find(|x| &x.0 == c).map(|x| x.1.clone()).unwrap_or(Value::Null)).collect()).collect();
    QueryOut { columns: cols.into_iter().map(|name| Column { name, type_name: String::new() }).collect(), rows, truncated: items.len() > limit, has_result_set: true, rows_affected: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_requests() {
        assert_eq!(parse_request("GET /collections").unwrap(), ("GET".into(), "/collections".into(), None));
        let (m, p, b) = parse_request("post collections/x/points/search\n{vector: [0.1, 0.2], limit: 3,}").unwrap();
        assert_eq!((m.as_str(), p.as_str()), ("POST", "/collections/x/points/search"));
        assert_eq!(b.unwrap(), json!({ "vector": [0.1, 0.2], "limit": 3 }));
        assert!(parse_request("select 1").is_err());
    }

    #[test]
    fn flattens_results() {
        let v = json!([{ "id": 1, "score": 0.9, "payload": { "city": "Seoul" }, "vector": (0..10).map(|i| i as f64).collect::<Vec<_>>() }, { "id": 2, "score": 0.5, "payload": { "city": "Busan", "n": 3 } }]);
        let t = table(&v, &["payload"], 10);
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "payload.city", "score", "vector", "payload.n"]);
        assert_eq!(t.rows[0][3], json!("[0.0, 1.0, 2.0, 3.0, … (10)]"));
        assert_eq!(t.rows[1][4], json!(3));
    }
}
