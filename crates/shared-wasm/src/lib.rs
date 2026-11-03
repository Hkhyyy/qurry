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

/// The result grid's line when loading more rows failed (the TUI's words)
#[wasm_bindgen(js_name = moreRowsFailed)]
pub fn more_rows_failed(msg: &str) -> String {
    qurry_shared::errors::more_rows_failed(msg)
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

    /// The value · json key spot at the caret (what to ask meta.hints for), or undefined
    #[wasm_bindgen(js_name = hintAt)]
    pub fn hint_at(&self, text: &str, caret: usize, db: &str, mysql: bool) -> Result<JsValue, JsError> {
        let at = Caret::new(text, caret);
        Ok(serde_wasm_bindgen::to_value(&complete::hint_at(&self.meta, &at.statement(text, mysql), at.before, db, mysql))?)
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

/// The range and the leading candidates (the statement's columns), and what the rest comes from
fn suggestions(m: &complete::Meta, text: &str, caret: usize, db: &str, mysql: bool) -> (Suggestions, Option<complete::Rest>) {
    let at = Caret::new(text, caret);
    let (n, items, rest) = complete::suggest_lead(m, &at.statement(text, mysql), at.before, db, mysql, Options::GUI);
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
        qurry_shared::sql::statement_at(text, self.at, if mysql { DbKind::Mysql } else { DbKind::Postgres }).unwrap_or_default()
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

/// Alignment of a value class ("left", "right", "center") given the saved overrides ({ null: "left", … })
#[wasm_bindgen(js_name = alignOf)]
pub fn align_of(class: JsValue, overrides: JsValue) -> Result<JsValue, JsError> {
    to(&qurry_shared::format::align(from(class)?, &qurry_shared::format::alignments(&from(overrides)?)))
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
        // byte_ranges reads every offset as byte_offset does, in order or not
        let offs = [0, 9, 11, 12, 13, 15, caret, 10_000, 3];
        let pairs: Vec<(usize, usize)> = offs.windows(2).map(|w| (w[0], w[1])).collect();
        assert_eq!(byte_ranges(t, &pairs), pairs.iter().map(|&(a, b)| (byte_offset(t, a), byte_offset(t, b))).collect::<Vec<_>>());
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
