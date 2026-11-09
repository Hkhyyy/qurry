//! Thin wasm-bindgen layer over `qurry-shared`: each function takes and returns plain JS values
//! (converted with serde-wasm-bindgen). Built by `pnpm wasm` into `src/wasm/pkg`.
//! No logic here except offsets: Rust works in UTF-8 byte offsets, JS strings in UTF-16 code units, so ranges and
//! caret positions (and CompletionIndex's Monaco columns) are converted at this boundary. `kind` is a DbKind as stored in connections.json ("postgres", "mysql" …).
//!
//! The statement checks here (is_read, is_ddl, unsafe_statement) are for UI decisions only: badges, confirm dialogs,
//! which statements a run counts as reads. They are not a security boundary. Enforcement (read-only Data Sources,
//! MCP read-only and execute) still runs in the backend, through qurry_core::guard, on every statement it receives.

use qurry_shared::exec::{self, ExecSettings, Plan};
use qurry_shared::DbKind;
use qurry_shared::complete::{self, Options};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use qurry_shared::chart::{self, Column};
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

/// UI only: why Explain Analyse asks before it runs the statement, or undefined (the TUI asks with the same words)
#[wasm_bindgen(js_name = explainAnalyzeAsk)]
pub fn explain_analyze_ask(sql: &str, kind: JsValue) -> Result<Option<String>, JsError> {
    Ok(qurry_shared::guard::explain_analyze_ask(sql, parse_kind(kind)?))
}

/// UI only: what Output says after Explain Analyse rolled the statement back, [warn, text after "Explain Analyse "], or undefined
#[wasm_bindgen(js_name = explainAnalyzeNote)]
pub fn explain_analyze_note(sql: &str, kind: JsValue) -> Result<JsValue, JsError> {
    to(&qurry_shared::guard::explain_analyze_note(sql, parse_kind(kind)?))
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

/// The data source form's hint for a local Host with AWS SSM (the instance itself), or undefined (the TUI's words)
#[wasm_bindgen(js_name = ssmLocalHost)]
pub fn ssm_local_host(host: &str) -> Option<String> {
    qurry_shared::errors::ssm_local_host(host)
}

/// The DBA report's database picker when the databases can't be listed (the TUI's words)
#[wasm_bindgen(js_name = dbListFailed)]
pub fn db_list_failed(msg: &str) -> String {
    qurry_shared::errors::db_list_failed(msg)
}

/// The result grid's line when loading more rows failed (the TUI's words)
#[wasm_bindgen(js_name = moreRowsFailed)]
pub fn more_rows_failed(msg: &str) -> String {
    qurry_shared::errors::more_rows_failed(msg)
}

/// Where a failed statement's error points in `sql` (the text sent), as a UTF-16 offset, or undefined when the error doesn't say
#[wasm_bindgen(js_name = errorPosition)]
pub fn error_position(msg: &str, sql: &str) -> Option<usize> {
    qurry_shared::errors::error_position(msg, sql).map(|b| sql[..b].encode_utf16().count())
}

/// A failed statement's error without the position the core adds to it, for its mark's hover (the mark shows the spot)
#[wasm_bindgen(js_name = withoutPosition)]
pub fn without_position(msg: &str) -> String {
    qurry_shared::errors::without_position(msg).to_string()
}

/// The result grid's shown columns ("cols 3–8 of 14"), or undefined when every column fits (the TUI's words)
#[wasm_bindgen(js_name = shownCols)]
pub fn shown_cols(first: usize, last: usize, total: usize) -> Option<String> {
    qurry_shared::format::shown_cols(first, last, total)
}

/// A join result's cell on the outer side where no row of `table` matched: why it is read-only (the TUI's words)
#[wasm_bindgen(js_name = editNoRow)]
pub fn edit_no_row(table: &str) -> String {
    qurry_shared::errors::edit_no_row(table)
}

/// Why Add / Delete / Clone Row are greyed out on a join result (the TUI's words)
#[wasm_bindgen(js_name = editJoinRows)]
pub fn edit_join_rows() -> String {
    qurry_shared::errors::EDIT_JOIN_ROWS.into()
}

/// What a Production console that starts in Manual mode says the first time (the TUI's words)
#[wasm_bindgen(js_name = productionManual)]
pub fn production_manual() -> String {
    qurry_shared::errors::PRODUCTION_MANUAL.into()
}

/// Title of the notice for a failed Commit: "Commit outcome unknown" when the connection was lost during it, else "Commit failed"
#[wasm_bindgen(js_name = commitFailedTitle)]
pub fn commit_failed_title(msg: &str) -> String {
    qurry_shared::errors::commit_failed_title(msg).into()
}

/// Result word of a Production log entry ("outcome unknown", "failed", "committed" …), or undefined when its rows say it
#[wasm_bindgen(js_name = auditStatus)]
pub fn audit_status(started: bool, sql: &str, failed: bool) -> Option<String> {
    qurry_shared::audit::status(started, sql, failed).map(String::from)
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
    to(&chart::numeric_columns(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, chart::Options::GUI))
}

/// Chart: initial axes { x, y } (y undefined = the row count)
#[wasm_bindgen(js_name = defaultAxes)]
pub fn default_axes(columns: JsValue, rows: JsValue) -> Result<JsValue, JsError> {
    to(&chart::default_axes(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, chart::Options::GUI))
}

/// Chart: Y summed per X value ({ labels, series }); a negative Y index is the row count
#[wasm_bindgen]
pub fn aggregate(columns: JsValue, rows: JsValue, x: usize, ys: JsValue) -> Result<JsValue, JsError> {
    let ys: Vec<Option<usize>> = from::<Vec<i64>>(ys)?.into_iter().map(|y| usize::try_from(y).ok()).collect();
    to(&chart::aggregate(&from::<Vec<Column>>(columns)?, &from::<Vec<Vec<Value>>>(rows)?, x, &ys, chart::Options::GUI))
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

/// Statements split on `;`: [[start, end], …] in UTF-16 code units. `opts` is a SplitOptions object ({} = as the server reads them)
#[wasm_bindgen(js_name = splitStatements)]
pub fn split_statements(text: &str, kind: JsValue, opts: JsValue) -> Result<JsValue, JsError> {
    let r = utf16_ranges(text, &qurry_shared::sql::split_statements_with(text, parse_kind(kind)?, serde_wasm_bindgen::from_value(opts)?));
    Ok(serde_wasm_bindgen::to_value(&r)?)
}

/// Editor statements (also split at statement keywords on new lines without `;`): [[start, end], …] in UTF-16 code units
#[wasm_bindgen(js_name = splitScript)]
pub fn split_script(text: &str, kind: JsValue, opts: JsValue) -> Result<JsValue, JsError> {
    let r = utf16_ranges(text, &qurry_shared::sql::split_script_with(text, parse_kind(kind)?, serde_wasm_bindgen::from_value(opts)?));
    Ok(serde_wasm_bindgen::to_value(&r)?)
}

/// A statement range as the GUI holds it (UTF-16 code units)
#[derive(Deserialize)]
struct Segment {
    start: usize,
    end: usize,
}

#[derive(Serialize)]
struct JsStmt {
    start: usize,
    end: usize,
    sql: String,
}

#[derive(Serialize)]
struct JsChoice {
    label: String,
    stmts: Vec<JsStmt>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum JsPlan {
    Run(Vec<JsStmt>),
    Ask(Vec<JsChoice>),
}

/// Statements with UTF-16 ranges in `text`
fn js_stmts(text: &str, v: Vec<exec::Stmt>) -> Vec<JsStmt> {
    let r = utf16_ranges(text, &v.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>());
    v.into_iter().zip(r).map(|(s, [start, end])| JsStmt { start, end, sql: s.sql }).collect()
}

/// What to run from the caret (qurry_shared::exec::plan_in) over `segs` (the console's statements, UTF-16 ranges in order) with the
/// Query Execution settings ({inside, outside, selection}; unknown values fall back to the defaults): {run} | {ask} | undefined
#[wasm_bindgen(js_name = planExecution)]
pub fn plan_execution(text: &str, offset: usize, segs: JsValue, kind: JsValue, settings: JsValue) -> Result<JsValue, JsError> {
    let segs: Vec<Segment> = serde_wasm_bindgen::from_value(segs)?;
    let segs = byte_ranges(text, &segs.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>());
    let settings = ExecSettings::from_json(&serde_wasm_bindgen::from_value(settings)?);
    let js = |v| js_stmts(text, v);
    let plan = match exec::plan_in(text, byte_offset(text, offset), &segs, parse_kind(kind)?, settings) {
        Plan::Nothing => None,
        Plan::Run(v) => Some(JsPlan::Run(js(v))),
        Plan::Ask(opts) => Some(JsPlan::Ask(opts.into_iter().map(|(label, v)| JsChoice { label, stmts: js(v) }).collect())),
    };
    Ok(serde_wasm_bindgen::to_value(&plan)?)
}

/// What to run from a selection (qurry_shared::exec::plan_selection_in) over `segs` (its statements, UTF-16 ranges in the selection)
/// with the Query Execution settings: [{start, end, sql}, …], ranges in the selection
#[wasm_bindgen(js_name = planSelection)]
pub fn plan_selection(sel: &str, segs: JsValue, settings: JsValue) -> Result<JsValue, JsError> {
    let segs: Vec<Segment> = serde_wasm_bindgen::from_value(segs)?;
    let segs = byte_ranges(sel, &segs.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>());
    let v = exec::plan_selection_in(sel, &segs, ExecSettings::from_json(&serde_wasm_bindgen::from_value(settings)?));
    Ok(serde_wasm_bindgen::to_value(&js_stmts(sel, v))?)
}

/// A value as a string literal `kind` reads (qurry_shared::sql::literal; null → NULL): the grid's filters and edit SQL, as its copy formats
#[wasm_bindgen(js_name = sqlLiteral)]
pub fn sql_literal(kind: JsValue, value: Option<String>) -> Result<String, JsError> {
    Ok(qurry_shared::sql::literal(parse_kind(kind)?, value.as_deref()))
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
    /// The last columns of the statement's tables and their JS array, and the same for the candidates after them (tables,
    /// schemas, keywords, by what they come from). Both stay the same while typing (Monaco filters them), so thousands of
    /// candidates are built and cross the boundary once instead of on every key
    lead: Memo<Vec<complete::Item>>,
    rest: Memo<complete::Rest>,
}

type Memo<K> = RefCell<Option<(K, JsValue)>>;

/// The JS value for `key`: the last one when the key is the same, else `make`'s (kept for next time)
fn reuse<K: PartialEq>(memo: &Memo<K>, key: K, make: impl FnOnce(&K) -> Result<JsValue, JsError>) -> Result<JsValue, JsError> {
    let mut memo = memo.borrow_mut();
    if let Some((prev, js)) = &*memo
        && *prev == key
    {
        return Ok(js.clone());
    }
    let js = make(&key)?;
    *memo = Some((key, js.clone()));
    Ok(js)
}

/// suggest's answer: the range on the caret's line, then the candidates as `lead` (columns) + `rest`, each reused while unchanged
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

/// suggestFilter's answer
#[derive(Serialize)]
struct FilterSuggestions {
    from: usize,
    items: Vec<complete::Item>,
}

#[wasm_bindgen]
impl CompletionIndex {
    /// `tables_json`: the completion_meta result ([{schema, name, columns: [[name, type], …]}, …]) as JSON
    #[wasm_bindgen(constructor)]
    pub fn new(tables_json: &str) -> Result<CompletionIndex, JsError> {
        let tables = serde_json::from_str(tables_json).map_err(|e| JsError::new(&format!("Completion metadata is not in the expected shape ({e})")))?;
        Ok(CompletionIndex { meta: complete::Meta::new(tables), lead: RefCell::new(None), rest: RefCell::new(None) })
    }

    /// Candidates at the caret (a UTF-16 offset into `text`, the console), replacing the word before it; tables and aliases are the
    /// caret's statement's. `db` is the tab's DB ("" for none).
    /// {startColumn, endColumn, lead, rest}: the candidates are lead then rest (either may be the same array as last time; don't change them)
    pub fn suggest(&self, text: &str, caret: usize, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        let (s, rest) = suggestions(&self.meta, text, caret, db, mysql);
        let lead = reuse(&self.lead, s.items, |items| Ok(serde_wasm_bindgen::to_value(items)?))?;
        let rest = match rest {
            None => serde_wasm_bindgen::to_value(&Vec::<complete::Item>::new())?,
            Some(r) => reuse(&self.rest, r, |r| Ok(serde_wasm_bindgen::to_value(&complete::rest_items(&self.meta, r, Options::GUI))?))?,
        };
        Ok(serde_wasm_bindgen::to_value(&SuggestJs { start_column: s.start_column, end_column: s.end_column, lead, rest })?)
    }

    /// Candidates in a table tab's WHERE / ORDER BY field (complete::suggest_filter) at `caret`, a UTF-16 offset into `field`;
    /// `explicit` when asked for (⌃Space) rather than typed. {from, items}: the word they replace starts at `from` (UTF-16) and ends at the caret
    #[wasm_bindgen(js_name = suggestFilter)]
    #[allow(clippy::too_many_arguments)]
    pub fn suggest_filter(&self, schema: &str, table: &str, order_by: bool, field: &str, caret: usize, db: &str, mysql: bool, explicit: bool) -> Result<JsValue, JsError> {
        let b = byte_offset(field, caret);
        let (n, items) = complete::suggest_filter(&self.meta, schema, table, order_by, field, b, db, mysql, explicit);
        Ok(serde_wasm_bindgen::to_value(&FilterSuggestions { from: caret - utf16_tail(&field[..b], n), items })?)
    }

    /// The value · json key spot at the caret (what to ask meta.hints for), or undefined
    #[wasm_bindgen(js_name = hintAt)]
    pub fn hint_at(&self, text: &str, caret: usize, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        let at = Caret::new(text, caret);
        Ok(serde_wasm_bindgen::to_value(&complete::hint_at(&self.meta, &at.statement(text, mysql), at.head(text, mysql), db, mysql))?)
    }

    /// The function call around the caret (a UTF-16 offset into the console): {signatures: [{label, params}], active}, or undefined
    #[wasm_bindgen(js_name = signatureAt)]
    pub fn signature_at(&self, text: &str, caret: usize, mysql: bool) -> Result<JsValue, JsError> {
        let (stmt, c) = complete::at_caret(text, byte_offset(text, caret), kind(mysql));
        Ok(serde_wasm_bindgen::to_value(&complete::signature(&self.meta, &stmt, c, mysql))?)
    }

    /// Tables and columns of the console's statements that their scope lacks (complete::script_marks):
    /// [{start, end, message, fixes}], start and end UTF-16 offsets into `text`
    pub fn marks(&self, text: &str, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&utf16_marks(text, complete::script_marks(&self.meta, text, kind(mysql), db)))?)
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
pub fn hint_insert(value: &str, ctx: JsValue, type_name: &str, mysql: bool) -> Result<String, JsError> {
    let c: complete::Ctx = serde_wasm_bindgen::from_value(ctx)?;
    Ok(complete::hint_insert(value, c.mode, &c.path, type_name, c.quoted, false, mysql))
}

/// Text replacing the operator at a keys spot with no keys: ` @> '[{…}]'`
#[wasm_bindgen(js_name = pairsInsert)]
pub fn pairs_insert(value: &str, op: &str) -> String {
    complete::pairs_insert(value, op)
}

/// A quote / bracket edit (qurry_shared::pairs::Edit) in UTF-16 code units: replace start..end of the text with `insert`, then
/// select selStart..selEnd of the new text
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairEdit {
    start: usize,
    end: usize,
    insert: String,
    sel_start: usize,
    sel_end: usize,
}

/// What a key does to quotes and brackets in a field outside Monaco (the table tab's WHERE / ORDER BY): `key` is the typed
/// character or "Backspace", start..end the selection (UTF-16). Undefined: the key edits as usual
#[wasm_bindgen(js_name = pairEdit)]
pub fn pair_edit(text: &str, start: usize, end: usize, key: &str, kind: JsValue) -> Result<JsValue, JsError> {
    let kind = parse_kind(kind)?;
    let (a, b) = (byte_offset(text, start), byte_offset(text, end));
    let mut chars = key.chars();
    let e = match (chars.next(), chars.next()) {
        _ if key == "Backspace" && a == b => qurry_shared::pairs::backspace(text, a, kind),
        (Some(ch), None) => qurry_shared::pairs::typed(text, (a, b), ch, kind),
        _ => None,
    };
    let Some(e) = e else { return Ok(JsValue::UNDEFINED) };
    let new = format!("{}{}{}", &text[..e.start], e.insert, &text[e.end..]);
    let u16_at = |t: &str, b: usize| t[..b].encode_utf16().count();
    to(&PairEdit { start: u16_at(text, e.start), end: u16_at(text, e.end), insert: e.insert, sel_start: u16_at(&new, e.sel.0), sel_end: u16_at(&new, e.sel.1) })
}

/// The quotes and brackets left open, and closing brackets without an opener (qurry_shared::pairs::unbalanced):
/// [{start, end, message}] in UTF-16 code units, in text order
#[wasm_bindgen]
pub fn unbalanced(text: &str, kind: JsValue) -> Result<JsValue, JsError> {
    #[derive(Serialize)]
    struct Mark {
        start: usize,
        end: usize,
        message: String,
    }
    let v = qurry_shared::pairs::unbalanced(text, parse_kind(kind)?);
    let r = utf16_ranges(text, &v.iter().map(|u| (u.start, u.end)).collect::<Vec<_>>());
    to(&v.into_iter().zip(r).map(|(u, [start, end])| Mark { start, end, message: u.message }).collect::<Vec<_>>())
}

/// The range and the leading candidates (the statement's columns), and what the rest comes from
fn suggestions(m: &complete::Meta, text: &str, caret: usize, db: &str, mysql: bool) -> (Suggestions, Option<complete::Rest>) {
    let at = Caret::new(text, caret);
    let (stmt, c) = complete::at_caret(text, at.at, kind(mysql));
    let (n, items, rest) = complete::suggest_lead(m, &stmt, c, db, mysql, Options::GUI);
    let end = at.column();
    (Suggestions { start_column: end - utf16_tail(at.before, n), end_column: end, items }, rest)
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
    /// Byte offset in the text
    at: usize,
}

impl<'a> Caret<'a> {
    fn new(text: &'a str, utf16: usize) -> Self {
        let b = byte_offset(text, utf16);
        let start = text[..b].rfind('\n').map_or(0, |i| i + 1);
        Caret { before: &text[start..b], after: text[b..].chars().next(), at: b }
    }

    /// The statement the caret is in (as the TUI passes it): an alias of another statement in the console doesn't resolve here
    fn statement(&self, text: &str, mysql: bool) -> String {
        qurry_shared::sql::statement_at(text, self.at, kind(mysql)).unwrap_or_default()
    }

    /// The caret's statement up to the caret (what hint_at reads the clause from)
    fn head<'t>(&self, text: &'t str, mysql: bool) -> &'t str {
        qurry_shared::sql::statement_head(text, self.at, kind(mysql))
    }

    /// Monaco column of the caret (1-based, UTF-16)
    fn column(&self) -> usize {
        self.before.encode_utf16().count() + 1
    }
}

/// Marks with UTF-16 offsets instead of byte offsets
fn utf16_marks(text: &str, mut v: Vec<complete::Mark>) -> Vec<complete::Mark> {
    let ranges: Vec<(usize, usize)> = v.iter().map(|m| (m.start, m.end)).collect();
    for (m, [a, b]) in v.iter_mut().zip(utf16_ranges(text, &ranges)) {
        (m.start, m.end) = (a, b);
    }
    v
}

/// The SQL dialect completion reads (MySQL, or PostgreSQL for everything else)
fn kind(mysql: bool) -> DbKind {
    if mysql { DbKind::Mysql } else { DbKind::Postgres }
}

/// UTF-16 length of the last `chars` characters of `s`
fn utf16_tail(s: &str, chars: usize) -> usize {
    s.chars().rev().take(chars).map(char::len_utf16).sum()
}

// Grid and search helpers. `opts` objects are the option structs of each qurry_shared module in camelCase ({} = the TUI's rules)

/// Time column (fractional seconds are aligned): time types except INTERVAL
#[wasm_bindgen(js_name = isTimeType)]
pub fn is_time_type(type_name: &str) -> bool {
    qurry_shared::format::is_time_type(type_name)
}

/// The longest fractional-second digit count among a column's text values (0 if none)
#[wasm_bindgen(js_name = fracWidth)]
pub fn frac_width(values: JsValue) -> Result<usize, JsError> {
    let v: Vec<String> = serde_wasm_bindgen::from_value(values)?;
    Ok(qurry_shared::format::frac_width(v.iter().map(String::as_str)))
}

/// A set column width (characters) moved by `steps` narrow (-) / widen (+) steps, kept in range (the TUI's < > =)
#[wasm_bindgen(js_name = columnWidth)]
pub fn column_width(width: f64, steps: f64) -> u16 {
    qurry_shared::format::column_width(width.round() as i64, steps as i64)
}

/// Fit: the characters the widest of `values` takes, East Asian wide characters and emoji counting 2 (the TUI's rule)
#[wasm_bindgen(js_name = textWidth)]
pub fn text_width(values: JsValue) -> Result<usize, JsError> {
    let v: Vec<String> = serde_wasm_bindgen::from_value(values)?;
    Ok(v.iter().map(|s| qurry_shared::format::text_width(s)).max().unwrap_or(0))
}

/// The keys a result's columns keep their set widths under (a repeated name gets its occurrence: `id#2`)
#[wasm_bindgen(js_name = columnKeys)]
pub fn column_keys(names: JsValue) -> Result<JsValue, JsError> {
    let v: Vec<String> = serde_wasm_bindgen::from_value(names)?;
    to(&qurry_shared::format::column_keys(v.iter().map(String::as_str)))
}

#[derive(Serialize)]
struct Reformatted {
    start: usize,
    end: usize,
    text: String,
}

/// Reformat Code (⌘⌥l): the selection (`sel_start` < `sel_end`, UTF-16) or the statement at `caret`, keywords in `case`
/// ("off" | "upper" | "lower"); { start, end, text } with UTF-16 offsets, or undefined when it changes nothing
#[wasm_bindgen]
pub fn reformat(text: &str, sel_start: usize, sel_end: usize, caret: usize, kind: JsValue, case: JsValue) -> Result<JsValue, JsError> {
    let sel = (sel_start < sel_end).then(|| (byte_offset(text, sel_start), byte_offset(text, sel_end)));
    let r = qurry_shared::format::reformat(text, sel, byte_offset(text, caret), parse_kind(kind)?, from(case)?);
    match r {
        Some(r) => {
            let [[start, end]] = utf16_ranges(text, &[(r.start, r.end)])[..] else { unreachable!() };
            to(&Reformatted { start, end, text: r.text })
        }
        None => Ok(JsValue::UNDEFINED),
    }
}

#[derive(Serialize)]
struct Cased {
    start: usize,
    text: String,
}

/// Keyword case as you type: `end` (UTF-16) is where the character just typed sits; { start, text } replaces the keyword it ends
/// (`text[start..end]`), or undefined when there is none to change
#[wasm_bindgen(js_name = keywordCaseAt)]
pub fn keyword_case_at(text: &str, end: usize, kind: JsValue, case: JsValue) -> Result<JsValue, JsError> {
    match qurry_shared::format::keyword_case_at(text, byte_offset(text, end), parse_kind(kind)?, from(case)?) {
        // Keywords are ASCII: as many UTF-16 units as bytes
        Some((_, cased)) => to(&Cased { start: end - cased.len(), text: cased }),
        None => Ok(JsValue::UNDEFINED),
    }
}

/// 03:19:15 → 03:19:15.000000 (`width` fractional digits)
#[wasm_bindgen(js_name = padFrac)]
pub fn pad_frac(s: &str, width: usize) -> String {
    qurry_shared::format::pad_frac(s, width)
}

/// The Value Editor panel's text for a cell: { json, text } (qurry_shared::value::panel_text)
#[wasm_bindgen(js_name = panelText)]
pub fn panel_text(type_name: &str, value: &str) -> Result<JsValue, JsError> {
    to(&qurry_shared::value::panel_text(type_name, value))
}

/// The panel's "{ }" toggle: valid JSON between pretty and one line, undefined for anything else
#[wasm_bindgen(js_name = toggleJsonLayout)]
pub fn toggle_json_layout(text: &str) -> Option<String> {
    qurry_shared::value::toggle_layout(text)
}

/// What a panel edit writes to the cell: { Ok: text } or { Err: why it can't be applied }
#[wasm_bindgen(js_name = valueToApply)]
pub fn value_to_apply(type_name: &str, original: &str, edited: &str) -> Result<JsValue, JsError> {
    to(&qurry_shared::value::value_to_apply(type_name, original, edited))
}

/// Value class of a grid cell ("string", "number", "dateTime", "boolean", "null", "jsonBinary")
#[wasm_bindgen(js_name = valueClass)]
pub fn value_class(type_name: &str, value: JsValue) -> Result<JsValue, JsError> {
    to(&qurry_shared::format::value_class(type_name, &from(value)?))
}

/// JSON column (json, jsonb …): its text cells are colored
#[wasm_bindgen(js_name = isJsonType)]
pub fn is_json_type(type_name: &str) -> bool {
    qurry_shared::format::is_json_type(type_name)
}

/// Colored pieces of a JSON cell's first `max_chars` characters, flat for speed: [start, end, kind, …] in UTF-16 units,
/// kind as the index of qurry_shared::format::JsonKind
#[wasm_bindgen(js_name = jsonSpans)]
pub fn json_spans(text: &str, max_chars: usize) -> Vec<u32> {
    let spans = qurry_shared::format::json_spans(text, max_chars);
    let r = utf16_ranges(text, &spans.iter().map(|&(s, e, _)| (s, e)).collect::<Vec<_>>());
    r.into_iter().zip(spans).flat_map(|([s, e], (_, _, k))| [s as u32, e as u32, k as u32]).collect()
}

/// Alignment setting of a value class ("left", "right", "center"; NULL also "column") given the saved overrides ({ null: "left", … })
#[wasm_bindgen(js_name = alignOf)]
pub fn align_of(class: JsValue, overrides: JsValue) -> Result<JsValue, JsError> {
    to(&qurry_shared::format::align(from(class)?, &qurry_shared::format::alignments(&from(overrides)?)))
}

/// Where a cell of a value class sits in a column of `type_name` ("left", "right", "center"): a NULL on "column" takes the column's.
/// `first` = the column's first non-null value or null (an untyped column goes by it; a stand-in of the same kind will do)
#[wasm_bindgen(js_name = cellAlign)]
pub fn cell_align(type_name: &str, first: JsValue, class: JsValue, overrides: JsValue) -> Result<JsValue, JsError> {
    let o = qurry_shared::format::alignments(&from(overrides)?);
    let first: Value = from(first)?;
    to(&qurry_shared::format::cell_align(qurry_shared::format::column_class(type_name, std::iter::once(&first)), from(class)?, &o))
}

/// [{id, db, schema, table}, …] → a label per tab (schema.table or db.schema.table), null where the name is unique
#[wasm_bindgen(js_name = tableTabLabels)]
pub fn table_tab_labels(tabs: JsValue) -> Result<JsValue, JsError> {
    let tabs: Vec<qurry_shared::labels::TableTab> = serde_wasm_bindgen::from_value(tabs)?;
    Ok(serde_wasm_bindgen::to_value(&qurry_shared::labels::table_tab_labels(&tabs))?)
}

/// Numeric or boolean column type: the IN list leaves its number text bare
#[wasm_bindgen(js_name = isNumericType)]
pub fn is_numeric_type(type_name: &str) -> bool {
    qurry_shared::extract::numeric_type(type_name)
}

/// Rows of cells (strings, numbers, booleans, null) as a list to put after IN, strings as `kind` reads them; `types` one per column
#[wasm_bindgen(js_name = inList)]
pub fn in_list(rows: JsValue, types: JsValue, kind: JsValue, opts: JsValue) -> Result<String, JsError> {
    let rows: Vec<Vec<serde_json::Value>> = serde_wasm_bindgen::from_value(rows)?;
    let types: Vec<String> = serde_wasm_bindgen::from_value(types)?;
    Ok(qurry_shared::extract::in_list(&rows, &types, parse_kind(kind)?, serde_wasm_bindgen::from_value(opts)?).0)
}

/// Rows ({names, types, vals, keys, tableRef, header}) as text in a Data Extractor format ("CSV", "SQL Inserts" …) for `kind`
#[wasm_bindgen(js_name = extractRows)]
pub fn extract_rows(format: JsValue, rows: JsValue, kind: JsValue, opts: JsValue) -> Result<String, JsError> {
    let rows: qurry_shared::extract::Rows = serde_wasm_bindgen::from_value(rows)?;
    Ok(qurry_shared::extract::rows(serde_wasm_bindgen::from_value(format)?, &rows, parse_kind(kind)?, serde_wasm_bindgen::from_value(opts)?))
}

/// Search score of `text` for `query` (higher first), undefined if it doesn't match. `word_starts` is the one fuzzy option, passed
/// as a plain argument: search popups call this once per item on every keystroke
#[wasm_bindgen(js_name = fuzzyScore)]
pub fn fuzzy_score(query: &str, text: &str, word_starts: bool) -> Option<f64> {
    qurry_shared::fuzzy::score(query, text, qurry_shared::fuzzy::Options { word_starts })
}

/// Redis key names → folders split on ":": [{kind: "folder", name, path, count, children} | {kind: "key", item: index}, …]
#[wasm_bindgen(js_name = keyTree)]
pub fn key_tree(keys: JsValue, opts: JsValue) -> Result<JsValue, JsError> {
    let keys: Vec<String> = serde_wasm_bindgen::from_value(keys)?;
    Ok(serde_wasm_bindgen::to_value(&qurry_shared::keytree::key_tree(&keys, serde_wasm_bindgen::from_value(opts)?))?)
}

/// Redis Terminal: for the text before the caret, the commands the word can become (with arguments, summary, read/write), whether
/// it is a key name (completed from a bounded SCAN) and the arguments still to type (qurry_shared::redis_cmd::assist)
#[wasm_bindgen(js_name = redisAssist)]
pub fn redis_assist(before: &str) -> Result<JsValue, JsError> {
    to(&qurry_shared::redis_cmd::assist(before))
}

/// Redis Terminal: what `HELP <command>` prints (from the bundled command table), or undefined when the line isn't a HELP
#[wasm_bindgen(js_name = redisHelp)]
pub fn redis_help(line: &str) -> Option<String> {
    qurry_shared::redis_cmd::help(line)
}

/// Redis Terminal: the key names listed for the typed prefix ([{label, insert}], the typed key first, `insert` quoted as
/// redis-cli reads it) from a bounded SCAN's names
#[wasm_bindgen(js_name = redisKeyChoices)]
pub fn redis_key_choices(prefix: &str, names: Vec<String>) -> Result<JsValue, JsError> {
    to(&qurry_shared::redis_cmd::key_choices(prefix, &names))
}

/// Redis database picker: "db0 · 1,500 keys", the name alone when the count is unknown (the TUI's words)
#[wasm_bindgen(js_name = redisDbLabel)]
pub fn redis_db_label(name: &str, keys: f64) -> String {
    qurry_shared::redis_cmd::db_label(name, keys as i64)
}

/// Redis key list: a key's TTL ("1d 4h"), "—" without expiry (the TUI's words)
#[wasm_bindgen(js_name = redisTtl)]
pub fn redis_ttl(ms: f64) -> String {
    qurry_shared::redis_cmd::ttl_label(ms as i64)
}

/// Redis Terminal: the reply of the line as a table ({heads, rows}: a hash as field / value …), or undefined to print it as redis-cli does
#[wasm_bindgen(js_name = redisReplyTable)]
pub fn redis_reply_table(line: &str, reply: JsValue) -> Result<JsValue, JsError> {
    let reply: Value = serde_wasm_bindgen::from_value(reply)?;
    to(&qurry_shared::redis_cmd::reply_table(line, &reply))
}

/// Table column for each file column ("" = skip); `table` null is a new table (the file's names)
#[wasm_bindgen(js_name = autoMap)]
pub fn auto_map(file: JsValue, table: JsValue) -> Result<JsValue, JsError> {
    let file: Vec<String> = serde_wasm_bindgen::from_value(file)?;
    let table: Option<Vec<String>> = serde_wasm_bindgen::from_value(table)?;
    Ok(serde_wasm_bindgen::to_value(&qurry_shared::automap::auto_map(&file, table.as_deref()))?)
}

/// Key notation in on-screen text: ⌘/⌃/⌥ + a letter without Shift is lowercase ("⌘k")
#[wasm_bindgen(js_name = keyLabel)]
pub fn key_label(s: &str) -> String {
    qurry_shared::keymap::key_label(s)
}

/// GUI display of a spec ("Mod+Shift+L" → "⇧⌘L" on macOS, "Ctrl+Shift+L" elsewhere)
#[wasm_bindgen(js_name = formatSpec)]
pub fn format_spec(spec: &str, mac: bool) -> String {
    qurry_shared::keymap::format_spec(spec, mac)
}

/// Whether a spec can be a command shortcut
#[wasm_bindgen]
pub fn bindable(spec: &str) -> bool {
    qurry_shared::keymap::bindable(spec)
}

/// Whether a spec can be a GUI key in `scope` ("global" / "tree" / "editor" / "grid")
#[wasm_bindgen(js_name = bindableIn)]
pub fn bindable_in(spec: &str, scope: JsValue) -> Result<bool, JsError> {
    Ok(qurry_shared::keymap::bindable_in(spec, serde_wasm_bindgen::from_value(scope)?))
}

#[derive(serde::Deserialize)]
struct Bound {
    id: String,
    scope: qurry_shared::keymap::Scope,
    keys: Vec<String>,
}

/// Commands other than `id` that use `spec` in an overlapping scope. `bound`: [{ id, scope, keys }] for every command
#[wasm_bindgen(js_name = keyConflicts)]
pub fn key_conflicts(id: String, scope: JsValue, spec: &str, bound: JsValue) -> Result<Vec<String>, JsError> {
    let bound: Vec<Bound> = serde_wasm_bindgen::from_value(bound)?;
    let scope = serde_wasm_bindgen::from_value(scope)?;
    Ok(qurry_shared::keymap::conflicts(&id, scope, spec, bound.iter().map(|b| (b.id.clone(), b.scope, b.keys.as_slice()))))
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

/// UTF-16 ranges → byte ranges (each end as byte_offset reads it). One pass while they come in order, like utf16_ranges
fn byte_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut it = text.char_indices().peekable();
    let mut units = 0;
    let mut at = |u: usize| {
        if u < units {
            (it, units) = (text.char_indices().peekable(), 0);
        }
        while let Some(&(_, c)) = it.peek() {
            if units >= u {
                break;
            }
            units += c.len_utf16();
            it.next();
        }
        it.peek().map_or(text.len(), |&(b, _)| b)
    };
    ranges.iter().map(|&(s, e)| (at(s), at(e))).collect()
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

/// Theme editor: the tokens whose contrast pair a palette ({token: "#rrggbb"}) doesn't meet ([{token, fg, bg, ratio, min}])
#[wasm_bindgen(js_name = themeWarnings)]
pub fn theme_warnings(colors: JsValue) -> Result<JsValue, JsError> {
    to(&qurry_shared::theme::warnings(&from(colors)?))
}

/// A theme file's text → {id: "", name, base, colors} (colors a plain object), or the error saying why it is refused
#[wasm_bindgen(js_name = parseThemeFile)]
pub fn parse_theme_file(text: &str) -> Result<JsValue, JsError> {
    let t = qurry_shared::theme::parse_file(text).map_err(|e| JsError::new(&e))?;
    Ok(t.serialize(&serde_wasm_bindgen::Serializer::json_compatible())?)
}

/// The file Export… writes for a custom theme
#[wasm_bindgen(js_name = themeFileText)]
pub fn theme_file_text(theme: JsValue) -> Result<String, JsError> {
    Ok(qurry_shared::theme::file_text(&from(theme)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_spans_cross_as_utf16() {
        let t = "{\"😀\":\"한\",\"n\":1}";
        let units: Vec<u16> = t.encode_utf16().collect();
        let got: Vec<(String, u32)> = json_spans(t, 100).chunks(3).map(|c| (String::from_utf16(&units[c[0] as usize..c[1] as usize]).unwrap(), c[2])).collect();
        let want = [("{", 4), ("\"😀\"", 0), (":", 4), ("\"한\"", 1), (",", 4), ("\"n\"", 0), (":", 4), ("1", 2), ("}", 4)];
        assert_eq!(got, want.map(|(s, k)| (s.to_string(), k)));
    }

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
        // byte_ranges reads every offset as byte_offset does, in order or not
        let offs = [0, 9, 11, 12, 13, 15, caret, 10_000, 3];
        let pairs: Vec<(usize, usize)> = offs.windows(2).map(|w| (w[0], w[1])).collect();
        assert_eq!(byte_ranges(t, &pairs), pairs.iter().map(|&(a, b)| (byte_offset(t, a), byte_offset(t, b))).collect::<Vec<_>>());
    }

    fn index() -> complete::Meta {
        let t = |s: &str, n: &str, cols: &[(&str, &str)]| complete::CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect(), ..Default::default() };
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
        let (s, _) = suggestions(&m, &t, c, "", false);
        assert_eq!((s.start_column, s.end_column), (11, 12), "line 2: two spaces, '😀' (4 units), ' || ' (4), then 상 at column 11");
        assert!(s.items.iter().any(|i| i.label == "상태"));
        let (t, c) = caret("select e.‸ from \"😀 emoji\" e");
        let (s, _) = suggestions(&m, &t, c, "", false);
        assert_eq!(((s.start_column, s.end_column), s.items[0].label.as_str()), ((10, 10), "값"));
        let (t, c) = caret("select 주.번‸ from sales.주문 주");
        let (s, _) = suggestions(&m, &t, c, "", false);
        assert_eq!(((s.start_column, s.end_column), s.items.len()), ((10, 11), 2));
    }

    /// A mark's range is UTF-16 offsets into the console (😀 is two units), as Monaco's getPositionAt reads them
    #[test]
    fn marks_are_utf16_offsets() {
        let m = index();
        let t = "select '😀', 상태x from sales.주문";
        let v = utf16_marks(t, complete::script_marks(&m, t, DbKind::Postgres, ""));
        let units: Vec<u16> = t.encode_utf16().collect();
        assert_eq!(v.iter().map(|x| String::from_utf16(&units[x.start..x.end]).unwrap()).collect::<Vec<_>>(), ["상태x"]);
        assert_eq!(v[0].fixes, ["상태"]);
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
