//! Thin wasm-bindgen layer over `qurry-shared`: each function takes and returns plain JS values
//! (converted with serde-wasm-bindgen). Built by `pnpm wasm` into `src/wasm/pkg`.
//! No logic here except offsets: Rust works in UTF-8 byte offsets, JS strings in UTF-16 code units, so ranges and
//! caret positions are converted at this boundary. `kind` is a DbKind as stored in connections.json ("postgres", "mysql" …).
//!
//! The statement checks here (is_read, is_ddl, unsafe_statement) are for UI decisions only: badges, confirm dialogs,
//! which statements a run counts as reads. They are not a security boundary. Enforcement (read-only Data Sources,
//! MCP read-only and execute) still runs in the backend, through qurry_core::guard, on every statement it receives.

use qurry_shared::DbKind;
use qurry_shared::chart::{self, Column, Options};
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping(req: JsValue) -> Result<JsValue, JsError> {
    let req = serde_wasm_bindgen::from_value(req)?;
    Ok(serde_wasm_bindgen::to_value(&qurry_shared::ping(req))?)
}

fn parse_kind(v: JsValue) -> Result<DbKind, JsError> {
    Ok(serde_wasm_bindgen::from_value(v)?)
}

/// UI only, not a security check (the backend enforces read-only): the statement doesn't change data
#[wasm_bindgen(js_name = isRead)]
pub fn is_read(sql: &str, kind: JsValue) -> Result<bool, JsError> {
    Ok(qurry_shared::guard::is_read(sql, parse_kind(kind)?))
}

/// UI only, not a security check: the statement changes structure (reload the explorer tree)
#[wasm_bindgen(js_name = isDdl)]
pub fn is_ddl(sql: &str, kind: JsValue) -> Result<bool, JsError> {
    Ok(qurry_shared::guard::is_ddl(sql, parse_kind(kind)?))
}

/// UI only, not a security check: why the statement needs a confirmation before it runs (UPDATE/DELETE without WHERE, DROP, TRUNCATE …), or undefined
#[wasm_bindgen(js_name = unsafeStatement)]
pub fn unsafe_statement(sql: &str, kind: JsValue) -> Result<Option<String>, JsError> {
    Ok(qurry_shared::guard::unsafe_statement(sql, parse_kind(kind)?))
}

/// The connect error asks for "Allow unencrypted connection" (the GUI then offers to turn it on)
#[wasm_bindgen(js_name = tlsOptIn)]
pub fn tls_opt_in(msg: &str) -> bool {
    qurry_shared::errors::tls_opt_in(msg)
}

fn from<T: serde::de::DeserializeOwned>(v: JsValue) -> Result<T, JsError> {
    Ok(serde_wasm_bindgen::from_value(v)?)
}

fn to<T: serde::Serialize>(v: &T) -> Result<JsValue, JsError> {
    Ok(serde_wasm_bindgen::to_value(v)?)
}

/// Chart (GUI options): columns that can be charted as numbers, as indexes
#[wasm_bindgen(js_name = numericColumns)]
pub fn numeric_columns(columns: JsValue, rows: JsValue) -> Result<JsValue, JsError> {
    to(&chart::numeric_columns(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, Options::GUI))
}

/// Chart: initial axes { x, y } (y undefined = the row count)
#[wasm_bindgen(js_name = defaultAxes)]
pub fn default_axes(columns: JsValue, rows: JsValue) -> Result<JsValue, JsError> {
    to(&chart::default_axes(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, Options::GUI))
}

/// Chart: Y summed per X value ({ labels, series }); a negative Y index is the row count
#[wasm_bindgen]
pub fn aggregate(columns: JsValue, rows: JsValue, x: usize, ys: JsValue) -> Result<JsValue, JsError> {
    let ys: Vec<Option<usize>> = from::<Vec<i64>>(ys)?.into_iter().map(|y| usize::try_from(y).ok()).collect();
    to(&chart::aggregate(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, x, &ys, Options::GUI))
}

/// Chart: pie slices of aggregate's first series, the smallest folded into "Other" past maxSlices
#[wasm_bindgen]
pub fn pie(data: JsValue, max_slices: usize) -> Result<JsValue, JsError> {
    to(&chart::pie(&from(data)?, max_slices))
}

/// Chart: axis ticks in 1 · 2 · 5 × 10ⁿ steps (a plain array, not a Float64Array)
#[wasm_bindgen(js_name = niceTicks)]
pub fn nice_ticks(min: f64, max: f64, count: f64) -> Result<JsValue, JsError> {
    to(&chart::nice_ticks(min, max, count))
}

/// Chart: axis label (1,200 · 15.3K · 1.2M)
#[wasm_bindgen(js_name = shortNumber)]
pub fn short_number(n: f64) -> String {
    chart::short_number(n)
}

/// Statements split on `;` as the server reads them: [[start, end], …] in UTF-16 code units
#[wasm_bindgen(js_name = splitStatements)]
pub fn split_statements(text: &str, kind: JsValue) -> Result<JsValue, JsError> {
    let r = utf16_ranges(text, &qurry_shared::sql::split_statements(text, parse_kind(kind)?));
    Ok(serde_wasm_bindgen::to_value(&r)?)
}

/// Editor statements (also split at statement keywords on new lines without `;`): [[start, end], …] in UTF-16 code units
#[wasm_bindgen(js_name = splitScript)]
pub fn split_script(text: &str, kind: JsValue) -> Result<JsValue, JsError> {
    let r = utf16_ranges(text, &qurry_shared::sql::split_script(text, parse_kind(kind)?));
    Ok(serde_wasm_bindgen::to_value(&r)?)
}

/// Statement at the caret (a UTF-16 offset, as in a JS string), trimmed; undefined if the text has none
#[wasm_bindgen(js_name = statementAt)]
pub fn statement_at(text: &str, offset: usize, kind: JsValue) -> Result<Option<String>, JsError> {
    Ok(qurry_shared::sql::statement_at(text, byte_offset(text, offset), parse_kind(kind)?))
}

/// Byte ranges → UTF-16 ranges. split_* returns them in order, on char boundaries, so one pass over the text converts them all
fn utf16_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<[usize; 2]> {
    let (mut byte, mut units) = (0, 0);
    let mut at = |b: usize| {
        if b < byte {
            (byte, units) = (0, 0);
        }
        units +=text[byte..b].encode_utf16().count();
        byte = b;
        units
    };
    ranges.iter().map(|&(s, e)| [at(s), at(e)]).collect()
}

/// UTF-16 offset → byte offset (inside a surrogate pair: the end of that char; past the end: the text's length)
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (b, c) in text.char_indices() {
        if units >= utf16 {
            return b;
        }
        units += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_cross_as_utf16() {
        let t = "select '한글 😀'; select 2";
        let r = utf16_ranges(t, &qurry_shared::sql::split_statements(t, DbKind::Postgres));
        let units: Vec<u16> = t.encode_utf16().collect();
        let part = |[s, e]: [usize; 2]| String::from_utf16(&units[s..e]).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(part(r[0]), "select '한글 😀'");
        assert_eq!(part(r[1]), " select 2");
        // Caret in the second statement, given as a JS index (😀 is two units, 한 and 글 one each)
        let caret = t.encode_utf16().count() - 1;
        assert_eq!(qurry_shared::sql::statement_at(t, byte_offset(t, caret), DbKind::Postgres).as_deref(), Some("select 2"));
        assert_eq!(byte_offset(t, 0), 0);
        assert_eq!(byte_offset(t, 10_000), t.len());
        // Inside the surrogate pair of 😀 (units 11 and 12): the end of 😀
        let emoji = t.find('😀').unwrap();
        assert_eq!(byte_offset(t, 12), emoji + '😀'.len_utf8());
    }
}
