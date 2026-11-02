//! Thin wasm-bindgen layer over `qurry-shared`: each function takes and returns plain JS values
//! (converted with serde-wasm-bindgen). Built by `pnpm wasm` into `src/wasm/pkg`.
//! No logic here except offsets: Rust works in UTF-8 byte offsets, JS strings in UTF-16 code units, so ranges and
//! caret positions (and CompletionIndex's Monaco columns) are converted at this boundary. `kind` is a DbKind as stored in connections.json ("postgres", "mysql" …).
//!
//! The statement checks here (is_read, is_ddl, unsafe_statement) are for UI decisions only: badges, confirm dialogs,
//! which statements a run counts as reads. They are not a security boundary. Enforcement (read-only Data Sources,
//! MCP read-only and execute) still runs in the backend, through qurry_core::guard, on every statement it receives.

use qurry_shared::DbKind;
use qurry_shared::complete::{self, Options};
use serde::Serialize;
use std::cell::RefCell;
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

/// One connection's completion metadata, indexed once when it loads (src/lib/completion.ts keeps one per connection).
/// Answers synchronously for Monaco: Options::GUI rules, ranges as 1-based UTF-16 columns on the caret's line
#[wasm_bindgen]
pub struct CompletionIndex {
    meta: complete::Meta,
    /// The last candidates after the statement's columns (tables, schemas, keywords) and their JS array. They stay the same
    /// while typing (Monaco filters them), so thousands of tables cross the boundary once instead of on every key
    rest: RefCell<Option<(Vec<complete::Item>, JsValue)>>,
}

/// suggest's answer: the range on the caret's line, then the candidates as `lead` (columns, converted each time) + `rest` (reused)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestJs {
    start_column: usize,
    end_column: usize,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    lead: JsValue,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    rest: JsValue,
}

/// Candidates and the range they replace on the caret's line
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Suggestions {
    start_column: usize,
    end_column: usize,
    items: Vec<complete::Item>,
}

#[wasm_bindgen]
impl CompletionIndex {
    /// `tables_json`: the completion_meta result ([{schema, name, columns: [[name, type], …]}, …]) as JSON
    #[wasm_bindgen(constructor)]
    pub fn new(tables_json: &str) -> Result<CompletionIndex, JsError> {
        let tables = serde_json::from_str(tables_json).map_err(|e| JsError::new(&format!("Completion metadata is not in the expected shape ({e})")))?;
        Ok(CompletionIndex { meta: complete::Meta::new(tables), rest: RefCell::new(None) })
    }

    /// Candidates at the caret (a UTF-16 offset into `text`), replacing the word before it. `db` is the tab's DB ("" for none).
    /// {startColumn, endColumn, lead, rest}: the candidates are lead then rest (rest may be the same array as last time; don't change it)
    pub fn suggest(&self, text: &str, caret: usize, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        let mut s = suggestions(&self.meta, text, caret, db, mysql);
        let rest = s.items.split_off(s.items.iter().position(|i| i.kind != complete::ItemKind::Column).unwrap_or(s.items.len()));
        let mut memo = self.rest.borrow_mut();
        let rest = match &*memo {
            Some((prev, js)) if *prev == rest => js.clone(),
            _ => {
                let js = serde_wasm_bindgen::to_value(&rest)?;
                *memo = Some((rest, js.clone()));
                js
            }
        };
        let lead = serde_wasm_bindgen::to_value(&s.items)?;
        Ok(serde_wasm_bindgen::to_value(&SuggestJs { start_column: s.start_column, end_column: s.end_column, lead, rest })?)
    }

    /// The value · json key spot at the caret (what to ask meta.hints for), or undefined
    #[wasm_bindgen(js_name = hintAt)]
    pub fn hint_at(&self, text: &str, caret: usize, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        let at = Caret::new(text, caret);
        Ok(serde_wasm_bindgen::to_value(&complete::hint_at(&self.meta, text, at.before, db, mysql))?)
    }

    /// What meta.hints returned for `hint` → candidates. as_pairs = keys were asked, none came, so these are @> conditions
    /// replacing the operator (and a closing quote right after the caret)
    #[wasm_bindgen(js_name = hintItems)]
    pub fn hint_items(&self, text: &str, caret: usize, hint: JsValue, hints: JsValue, as_pairs: bool) -> Result<JsValue, JsError> {
        let (h, hints) = (serde_wasm_bindgen::from_value(hint)?, serde_wasm_bindgen::from_value::<Vec<complete::Hint>>(hints)?);
        Ok(serde_wasm_bindgen::to_value(&hint_suggestions(text, caret, &h, &hints, as_pairs))?)
    }
}

/// The value spot the text before the cursor ends in ({ref, column, path, mode, quoted, typed, op}), or undefined
#[wasm_bindgen(js_name = hintContext)]
pub fn hint_context(before: &str, mysql: bool) -> Result<JsValue, JsError> {
    Ok(serde_wasm_bindgen::to_value(&complete::context(before, mysql))?)
}

/// Whether a column of this type gets value suggestions at that spot
#[wasm_bindgen]
pub fn hintable(ctx: JsValue, type_name: &str) -> Result<bool, JsError> {
    let c: complete::Ctx = serde_wasm_bindgen::from_value(ctx)?;
    Ok(complete::hintable(c.mode, &c.path, type_name))
}

/// Text inserted for a value at that spot (Monaco closes quotes itself, so none is added)
#[wasm_bindgen(js_name = hintInsert)]
pub fn hint_insert(value: &str, ctx: JsValue, type_name: &str) -> Result<String, JsError> {
    let c: complete::Ctx = serde_wasm_bindgen::from_value(ctx)?;
    Ok(complete::hint_insert(value, c.mode, &c.path, type_name, c.quoted, false))
}

/// Text replacing the operator at a keys spot with no keys: ` @> '[{…}]'`
#[wasm_bindgen(js_name = pairsInsert)]
pub fn pairs_insert(value: &str, op: &str) -> String {
    complete::pairs_insert(value, op)
}

fn suggestions(m: &complete::Meta, text: &str, caret: usize, db: &str, mysql: bool) -> Suggestions {
    let at = Caret::new(text, caret);
    let (n, items) = complete::suggest(m, text, at.before, db, mysql, Options::GUI);
    let end = at.column();
    Suggestions { start_column: end - utf16_tail(at.before, n), end_column: end, items }
}

fn hint_suggestions(text: &str, caret: usize, h: &complete::HintAt, hints: &[complete::Hint], as_pairs: bool) -> Suggestions {
    let at = Caret::new(text, caret);
    let items = complete::hint_items(h, hints, at.after, as_pairs, Options::GUI);
    let col = at.column();
    let (start_column, end_column) = if as_pairs { (col - h.op.encode_utf16().count(), col + usize::from(at.after == Some('\''))) } else { (col - h.typed.encode_utf16().count(), col) };
    Suggestions { start_column, end_column, items }
}

/// The caret's line up to the caret, and the character after it
struct Caret<'a> {
    before: &'a str,
    after: Option<char>,
}

impl<'a> Caret<'a> {
    fn new(text: &'a str, utf16: usize) -> Self {
        let b = byte_offset(text, utf16);
        let start = text[..b].rfind('\n').map_or(0, |i| i + 1);
        Caret { before: &text[start..b], after: text[b..].chars().next() }
    }

    /// Monaco column of the caret (1-based, UTF-16)
    fn column(&self) -> usize {
        self.before.encode_utf16().count() + 1
    }
}

/// UTF-16 length of the last `chars` characters of `s`
fn utf16_tail(s: &str, chars: usize) -> usize {
    s.chars().rev().take(chars).map(char::len_utf16).sum()
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

    fn index() -> complete::Meta {
        let t = |s: &str, n: &str, cols: &[(&str, &str)]| complete::CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect() };
        complete::Meta::new(vec![t("sales", "주문", &[("번호", "int4"), ("상태", "text")]), t("public", "😀 emoji", &[("값", "text")]), t("public", "orders", &[("reasons", "jsonb")])])
    }

    /// JS offset of the ‸ in `s`, and `s` without it
    fn caret(s: &str) -> (String, usize) {
        let at = s.find('‸').unwrap();
        (s.replacen('‸', "", 1), s[..at].encode_utf16().count())
    }

    /// Monaco columns count UTF-16 units: 😀 is two, 한 one; the range is the word before the caret on the caret's line
    #[test]
    fn suggest_ranges_are_utf16_columns() {
        let m = index();
        let (t, c) = caret("select '😀😀' as x,\n  '😀' || 상‸ from sales.주문");
        let s = suggestions(&m, &t, c, "", false);
        assert_eq!((s.start_column, s.end_column), (11, 12), "line 2: two spaces, '😀' (4 units), ' || ' (4), then 상 at column 11");
        assert!(s.items.iter().any(|i| i.label == "상태"));
        let (t, c) = caret("select e.‸ from \"😀 emoji\" e");
        let s = suggestions(&m, &t, c, "", false);
        assert_eq!(((s.start_column, s.end_column), s.items[0].label.as_str()), ((10, 10), "값"));
        let (t, c) = caret("select 주.번‸ from sales.주문 주");
        let s = suggestions(&m, &t, c, "", false);
        assert_eq!(((s.start_column, s.end_column), s.items.len()), ((10, 11), 2));
    }

    #[test]
    fn hint_ranges_are_utf16_columns() {
        let m = index();
        let (t, c) = caret("select '😀' a from sales.주문 where 상태 = '완‸'");
        let at = Caret::new(&t, c);
        let h = complete::hint_at(&m, &t, at.before, "", false).unwrap();
        let hints = [complete::Hint { value: "완료😀".into(), count: 1 }];
        let s = hint_suggestions(&t, c, &h, &hints, false);
        // 'select ' 7 + '😀' 4 + ' a from sales.주문 where 상태 = ' 28 + ' 1 = 40 units before 완
        assert_eq!((s.start_column, s.end_column, s.items[0].insert.as_str()), (41, 42, "완료😀"));
        // @> instead of keys: the operator and the auto-closed quote are replaced too
        let (t, c) = caret("select '😀' from orders where reasons ->> '‸'");
        let at = Caret::new(&t, c);
        let h = complete::hint_at(&m, &t, at.before, "", false).unwrap();
        let s = hint_suggestions(&t, c, &h, &[complete::Hint { value: r#"[{"k": "v"}]"#.into(), count: 2 }], true);
        let units: Vec<u16> = t.encode_utf16().collect();
        assert_eq!(String::from_utf16(&units[s.start_column - 1..s.end_column - 1]).unwrap(), " ->> ''");
        assert_eq!(s.items[0].insert, r#" @> '[{"k": "v"}]'"#);
    }
}
