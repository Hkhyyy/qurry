//! The result grid's Value Editor panel (GUI) and value panel (TUI): which values it shows as JSON, how, and what an edit writes back.
//! JSON is laid out again token by token, never parsed into numbers and printed: a jsonb bigint or 0.10 stays as the database sent it.

use crate::format::is_json_type;
use serde::Serialize;

fn valid_json(text: &str) -> Result<(), String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).map(|_| ()).map_err(|e| format!("Invalid JSON: {e}"))
}

/// A JSON object or array (what a text column holding JSON is shown as)
fn is_json_document(text: &str) -> bool {
    matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'[')) && valid_json(text).is_ok()
}

/// Valid JSON laid out again: `indent` spaces per level (as JSON.stringify(v, null, 2) does), or on one line without spaces when 0.
/// Only whitespace outside strings changes
pub fn layout_json(text: &str, indent: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut depth = 0usize;
    let newline = |out: &mut String, depth: usize| {
        if indent > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(depth * indent));
        }
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                out.push('"');
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '{' | '[' => {
                while chars.next_if(|c| c.is_whitespace()).is_some() {}
                out.push(ch);
                // {} and [] stay on one line
                if let Some(close) = chars.next_if(|&c| c == if ch == '{' { '}' } else { ']' }) {
                    out.push(close);
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(ch);
            }
            ',' => {
                out.push(',');
                newline(&mut out, depth);
            }
            ':' => out.push_str(if indent > 0 { ": " } else { ":" }),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    out
}

/// What the panel shows for a cell
#[derive(Debug, PartialEq, Serialize)]
pub struct PanelText {
    /// Shown as JSON (highlighted): a json / jsonb column, or a value that is a JSON object or array
    pub json: bool,
    /// Valid JSON pretty-printed, anything else as it is
    pub text: String,
}

pub fn panel_text(type_name: &str, value: &str) -> PanelText {
    let json = is_json_type(type_name) || is_json_document(value);
    let text = if json && valid_json(value).is_ok() { layout_json(value, 2) } else { value.to_string() };
    PanelText { json, text }
}

/// The "{ }" toggle: valid JSON between its pretty and its one-line layout; None for anything else
pub fn toggle_layout(text: &str) -> Option<String> {
    valid_json(text).ok()?;
    Some(layout_json(text, if text.contains('\n') { 0 } else { 2 }))
}

/// The value an edit made in the panel writes to the cell (`original` = the cell's value before, `edited` = the panel's text).
/// JSON keeps the cell's layout: a one-line value edited in the pretty layout the panel showed goes back on one line; a layout the user
/// changed is kept. A json / jsonb column refuses text that isn't JSON (Err = the message to show); a text column takes any text
pub fn value_to_apply(type_name: &str, original: &str, edited: &str) -> Result<String, String> {
    let json_column = is_json_type(type_name);
    if !json_column && !is_json_document(original) {
        return Ok(edited.to_string());
    }
    if let Err(e) = valid_json(edited) {
        return if json_column { Err(e) } else { Ok(edited.to_string()) };
    }
    Ok(if !original.contains('\n') && edited == layout_json(edited, 2) { layout_json(edited, 0) } else { edited.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_json_stringify_and_keeps_tokens() {
        let compact = r#"{"a":1,"b":[true,null,{"c":"x, y: {z}"}],"e":{},"f":[]}"#;
        let pretty = "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null,\n    {\n      \"c\": \"x, y: {z}\"\n    }\n  ],\n  \"e\": {},\n  \"f\": []\n}";
        assert_eq!(layout_json(compact, 2), pretty);
        assert_eq!(layout_json(pretty, 0), compact);
        // PostgreSQL's jsonb text (spaces after : and ,) and numbers a parse would round
        assert_eq!(layout_json(r#"{"n": 12345678901234567890, "p": 0.10, "s": "a \"q\" \\"}"#, 0), r#"{"n":12345678901234567890,"p":0.10,"s":"a \"q\" \\"}"#);
        assert_eq!(layout_json("[ ]", 2), "[]");
    }

    #[test]
    fn panel_shows_json_columns_and_json_documents_pretty() {
        assert_eq!(panel_text("JSONB", r#"{"a": [1, 2]}"#), PanelText { json: true, text: "{\n  \"a\": [\n    1,\n    2\n  ]\n}".into() });
        assert_eq!(panel_text("TEXT", "[1]"), PanelText { json: true, text: "[\n  1\n]".into() });
        // A jsonb scalar, a broken json value: JSON, as they are
        assert_eq!(panel_text("jsonb", "\"s\""), PanelText { json: true, text: "\"s\"".into() });
        assert_eq!(panel_text("json", "{oops"), PanelText { json: true, text: "{oops".into() });
        // Text that only looks like JSON, or a number, stays text
        assert_eq!(panel_text("TEXT", "{oops"), PanelText { json: false, text: "{oops".into() });
        assert_eq!(panel_text("INT8", "42"), PanelText { json: false, text: "42".into() });
    }

    #[test]
    fn apply_keeps_the_cells_layout() {
        let shown = panel_text("jsonb", r#"{"a": 1}"#).text;
        let edited = shown.replace('1', "2");
        // One line before, the panel's pretty layout edited: back on one line
        assert_eq!(value_to_apply("jsonb", r#"{"a": 1}"#, &edited), Ok(r#"{"a":2}"#.into()));
        // The user's own layout is kept
        assert_eq!(value_to_apply("jsonb", r#"{"a": 1}"#, "{ \"a\": 2 }"), Ok("{ \"a\": 2 }".into()));
        // Multi-line before: written as edited
        assert_eq!(value_to_apply("json", "{\n\"a\": 1\n}", &edited), Ok(edited.clone()));
        // A text column holding JSON gets the same rule
        assert_eq!(value_to_apply("TEXT", "[1,2]", "[\n  1,\n  3\n]"), Ok("[1,3]".into()));
        // Plain text is written as typed
        assert_eq!(value_to_apply("TEXT", "abc", "a\nb"), Ok("a\nb".into()));
    }

    #[test]
    fn invalid_json_is_refused_only_in_json_columns() {
        let e = value_to_apply("jsonb", "{}", "{\"a\": }").unwrap_err();
        assert!(e.starts_with("Invalid JSON: "), "{e}");
        assert!(value_to_apply("JSON", "", "").is_err());
        assert_eq!(value_to_apply("TEXT", "[1]", "[1,"), Ok("[1,".into()));
    }

    #[test]
    fn toggle_switches_between_pretty_and_one_line() {
        assert_eq!(toggle_layout(r#"{"a":1}"#), Some("{\n  \"a\": 1\n}".into()));
        assert_eq!(toggle_layout("{\n  \"a\": 1\n}"), Some(r#"{"a":1}"#.into()));
        assert_eq!(toggle_layout("nope"), None);
    }
}
