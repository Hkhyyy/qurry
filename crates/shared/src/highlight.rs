//! SQL coloring for one-line fields (#136: the table tab's WHERE / ORDER BY in the GUI and the TUI). It reads the tokens completion
//! reads (scope::lex) and completion's keyword list, so a word that completes as a keyword is drawn as one.

use crate::DbKind;
use crate::scope::{K, lex};
use serde::Serialize;

/// What a piece of text is drawn as
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    /// Completion's keywords, word operators included (IN, IS, NOT, NULL, AND, OR, LIKE, BETWEEN)
    Keyword,
    /// A name called: followed by `(`
    Function,
    Number,
    /// Quoted text: '…', E'…', $$…$$, MySQL "…"
    String,
    Comment,
    /// Names, quoted names and symbols (`=`, `<`, `(` …)
    Other,
}

/// A piece of `text` and its class: a byte range
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub class: Class,
}

/// The spans of `text` in order (whitespace between them belongs to none). An unclosed string or comment runs to the end
pub fn highlight(text: &str, kind: DbKind) -> Vec<Span> {
    let toks = lex(text, kind == DbKind::Mysql);
    let mut out = vec![];
    let mut at = 0;
    for (i, t) in toks.iter().enumerate() {
        comments(text, at, t.s, &mut out);
        at = t.e;
        let w = &text[t.s..t.e];
        let next = toks.get(i + 1);
        let class = match t.k {
            K::Num => Class::Number,
            K::Str => Class::String,
            K::Quoted | K::Punct => Class::Other,
            // E'…': the prefix is part of the string
            K::Word if w.eq_ignore_ascii_case("e") && next.is_some_and(|n| n.k == K::Str && n.s == t.e) => Class::String,
            K::Word if crate::complete::is_listed_keyword(w) => Class::Keyword,
            K::Word if next.is_some_and(|n| &text[n.s..n.e] == "(") => Class::Function,
            K::Word => Class::Other,
        };
        out.push(Span { start: t.s, end: t.e, class });
    }
    comments(text, at, text.len(), &mut out);
    out
}

/// lex skips only whitespace and comments, so what a gap between two tokens holds besides whitespace is comments
fn comments(text: &str, from: usize, to: usize, out: &mut Vec<Span>) {
    let gap = &text[from..to];
    let body = gap.trim_start();
    let start = from + gap.len() - body.len();
    let end = start + body.trim_end().len();
    if start < end {
        out.push(Span { start, end, class: Class::Comment });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Class::*;

    /// Each span's text and class, whitespace dropped
    fn spans(text: &str, kind: DbKind) -> Vec<(&str, Class)> {
        highlight(text, kind).into_iter().map(|s| (&text[s.start..s.end], s.class)).collect()
    }
    fn pg(text: &str) -> Vec<(&str, Class)> {
        spans(text, DbKind::Postgres)
    }

    #[test]
    fn highlight_word_operators_are_keywords_and_symbols_stay_other() {
        assert_eq!(pg("a = 1 AND b IN (2)"), [("a", Other), ("=", Other), ("1", Number), ("AND", Keyword), ("b", Other), ("IN", Keyword), ("(", Other), ("2", Number), (")", Other)]);
        let kw: Vec<_> = pg("x is not null or y like 'a' and z between 1 and 2").into_iter().filter(|s| s.1 == Keyword).map(|s| s.0).collect();
        assert_eq!(kw, ["is", "not", "null", "or", "like", "and", "between", "and"]);
        assert!(pg("a <> b >= c != d < e > f").iter().all(|s| s.1 == Other));
    }

    #[test]
    fn highlight_order_by_words() {
        assert_eq!(pg("created_at DESC NULLS LAST, id"), [("created_at", Other), ("DESC", Keyword), ("NULLS", Keyword), ("LAST", Keyword), (",", Other), ("id", Other)]);
    }

    #[test]
    fn highlight_functions_are_names_called() {
        assert_eq!(pg("lower(email) = now ()"), [("lower", Function), ("(", Other), ("email", Other), (")", Other), ("=", Other), ("now", Function), ("(", Other), (")", Other)]);
        // A keyword before ( stays a keyword, a name without ( a name
        assert_eq!(pg("id in (1) and count"), [("id", Other), ("in", Keyword), ("(", Other), ("1", Number), (")", Other), ("and", Keyword), ("count", Other)]);
    }

    #[test]
    fn highlight_numbers() {
        assert_eq!(pg("a > 1.5 and b < 10e3"), [("a", Other), (">", Other), ("1.5", Number), ("and", Keyword), ("b", Other), ("<", Other), ("10e3", Number)]);
        // A digit inside a name is the name's
        assert_eq!(pg("col1 = 2"), [("col1", Other), ("=", Other), ("2", Number)]);
    }

    #[test]
    fn highlight_strings_with_escapes() {
        // PostgreSQL: '' inside '…', \ escapes only in E'…', "…" is a name
        assert_eq!(pg(r#"a = 'it''s' or b = E'x\'y' or c = 'a\' or "Name" = $$z$$"#), [
            ("a", Other), ("=", Other), ("'it''s'", String), ("or", Keyword), ("b", Other), ("=", Other), ("E", String), (r"'x\'y'", String),
            ("or", Keyword), ("c", Other), ("=", Other), (r"'a\'", String), ("or", Keyword), ("\"Name\"", Other), ("=", Other), ("$$z$$", String),
        ]);
        // MySQL: \ escapes, "…" is a string and `…` a name
        assert_eq!(spans(r#"a = 'x\'y' or b = "q\"r" or `c` = 1"#, DbKind::Mysql), [
            ("a", Other), ("=", Other), (r"'x\'y'", String), ("or", Keyword), ("b", Other), ("=", Other), (r#""q\"r""#, String),
            ("or", Keyword), ("`c`", Other), ("=", Other), ("1", Number),
        ]);
    }

    #[test]
    fn highlight_comments() {
        assert_eq!(pg("a = 1 -- note\n and /* x */ b"), [("a", Other), ("=", Other), ("1", Number), ("-- note", Comment), ("and", Keyword), ("/* x */", Comment), ("b", Other)]);
        // MySQL: # comments, -- only before a space
        assert_eq!(spans("a = 1--1 # end", DbKind::Mysql), [("a", Other), ("=", Other), ("1", Number), ("-", Other), ("-", Other), ("1", Number), ("# end", Comment)]);
    }

    #[test]
    fn highlight_unclosed_string_and_comment_run_to_the_end() {
        assert_eq!(pg("status = 'paid and x"), [("status", Other), ("=", Other), ("'paid and x", String)]);
        assert_eq!(pg("a /* open"), [("a", Other), ("/* open", Comment)]);
    }

    #[test]
    fn highlight_empty_and_blank() {
        assert!(pg("").is_empty());
        assert!(pg("   ").is_empty());
    }

    #[test]
    fn highlight_multibyte_text() {
        let t = "이름 = '한글' AND 값 IN (1)";
        assert_eq!(spans(t, DbKind::Postgres), [("이름", Other), ("=", Other), ("'한글'", String), ("AND", Keyword), ("값", Other), ("IN", Keyword), ("(", Other), ("1", Number), (")", Other)]);
    }
}
