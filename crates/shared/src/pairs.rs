//! Quote and bracket pairs where the text isn't edited in Monaco: the table tab's WHERE / ORDER BY fields (GUI, through WASM)
//! and the TUI editor and filter inputs. Typing follows Monaco's SQL rules, which the GUI console keeps: `'` `"` `(` `[` close
//! themselves before a space, the line end or `;:.,=}])>` (Monaco's autoCloseBefore), a quote not right after a word character;
//! a closing character types over the one already there, ⌫ between an empty pair removes both, typing an opening one around a
//! selection wraps it. Unlike Monaco nothing closes itself inside a string or a comment.
//! `unbalanced` marks the quotes and brackets left open (and closing brackets without an opener), for the GUI console's
//! markers and the TUI editor. Offsets are UTF-8 bytes; the scan reads strings, comments and dollar quotes as the statement
//! splitter does (qurry_shared::sql).

use crate::sql::{backslash_escapes, bracket_names, dash_comment, dollar_opens, dollar_quotes, e_strings, escapes_in, hash_comment, is_dollar_tag, is_e_string};
use crate::DbKind;
use serde::Serialize;

/// The characters a pair closes itself before (Monaco's default autoCloseBefore), besides the end of the text
const CLOSE_BEFORE: &str = ";:.,=}])> \t\r\n";

/// Replace `start..end` with `insert`; afterwards `sel` is the selection (anchor, caret) in the new text
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub insert: String,
    pub sel: (usize, usize),
}

/// A quote or bracket left open, or a closing bracket that closes nothing: `start..end` is the character (the whole `$tag$`)
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Unbalanced {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

fn closer(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '\'' | '"' => Some(open),
        _ => None,
    }
}

/// What typing `ch` over the selection `sel` (anchor, caret; equal when nothing is selected) does differently from inserting
/// it, or None to insert it as usual
pub fn typed(text: &str, sel: (usize, usize), ch: char, kind: DbKind) -> Option<Edit> {
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let close = closer(ch);
    if a < b {
        let close = close?;
        return Some(Edit { start: a, end: b, insert: format!("{ch}{}{close}", &text[a..b]), sel: (a + 1, b + 1) });
    }
    let next = text[a..].chars().next();
    let at = scan(text, kind, a).at_end;
    let quote = ch == '\'' || ch == '"';
    let over = Edit { start: a, end: a + 1, insert: ch.to_string(), sel: (a + 1, a + 1) };
    // The closing quote of the string the caret is in, or a closing bracket outside strings and comments
    if next == Some(ch) && ((quote && at == At::Quote(ch as u8)) || (matches!(ch, ')' | ']') && at == At::Code)) {
        return Some(over);
    }
    let close = close?;
    let prev = text[..a].chars().next_back();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if at != At::Code || next.is_some_and(|c| !CLOSE_BEFORE.contains(c)) || (quote && prev.is_some_and(word)) {
        return None;
    }
    Some(Edit { start: a, end: a, insert: format!("{ch}{close}"), sel: (a + 1, a + 1) })
}

/// What ⌫ at `caret` (nothing selected) does differently from deleting one character: between an empty pair, deletes both
pub fn backspace(text: &str, caret: usize, kind: DbKind) -> Option<Edit> {
    let open = text[..caret].chars().next_back()?;
    let start = caret - open.len_utf8();
    let close = closer(open)?;
    if !text[caret..].starts_with(close) ||scan(text, kind, start).at_end != At::Code {
        return None;
    }
    Some(Edit { start, end: caret + close.len_utf8(), insert: String::new(), sel: (start, start) })
}

/// The quotes and brackets `text` leaves open and the closing brackets without an opener, in text order. Brackets pair up within
/// a statement (a `;` outside strings ends it). Nothing for Redis, MongoDB and the vector databases (not SQL)
pub fn unbalanced(text: &str, kind: DbKind) -> Vec<Unbalanced> {
    if matches!(kind, DbKind::Redis | DbKind::Mongodb) || kind.is_vector() {
        return vec![];
    }
    let s = scan(text, kind, text.len());
    let mut out: Vec<Unbalanced> = s.open.into_iter().map(|(i, c)| Unbalanced { start: i, end: i + 1, message: format!("Unclosed '{}'", c as char) }).collect();
    out.extend(s.strays.into_iter().map(|(i, c)| Unbalanced { start: i, end: i + 1, message: format!("'{}' closes nothing", c as char) }));
    if let Some((i, len)) = s.literal {
        let q = &text[i..i + len];
        let message = match s.at_end {
            At::Dollar => format!("Unclosed {q} quote"),
            At::Quote(b'\'') => "Unclosed string: no closing '".into(),
            _ => format!("Unclosed name: no closing {}", if q == "[" { "]" } else { q }),
        };
        out.push(Unbalanced { start: i, end: i + len, message });
    }
    out.sort_by_key(|u| u.start);
    out
}

/// What the scan is in at its end
#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    Code,
    /// A string or quoted name: its quote character (`[` for a [name])
    Quote(u8),
    Dollar,
    Comment,
}

struct Scan {
    at_end: At,
    /// The open quote at the end (its start and length), when `at_end` is Quote or Dollar
    literal: Option<(usize, usize)>,
    /// Brackets left open (where, which)
    open: Vec<(usize, u8)>,
    /// Closing brackets without an opener
    strays: Vec<(usize, u8)>,
}

/// Read `text` up to `stop`: what `stop` is inside, and the brackets
fn scan(text: &str, kind: DbKind, stop: usize) -> Scan {
    let b = text.as_bytes();
    let n = b.len();
    let mut s = Scan { at_end: At::Code, literal: None, open: vec![], strays: vec![] };
    let mut stack: Vec<(usize, u8)> = vec![];
    let backslash = backslash_escapes(kind);
    let mut i = 0;
    // A literal from `i` (opener `len` bytes) to `end` (past its closer; `closed` false: it runs to the end)
    let inside = |s: &mut Scan, i: usize, len: usize, end: usize, closed: bool, at: At| {
        let hit = stop >= i + len && (stop < end || !closed);
        if hit {
            s.at_end = at;
            if at != At::Comment {
                s.literal = Some((i, len));
            }
        }
        hit
    };
    while i < stop {
        let c = b[i];
        let d = if i + 1 < n { b[i + 1] } else { 0 };
        if dash_comment(b, i, n, kind) || (c == b'#' && hash_comment(kind, d)) {
            // Up to its line break (a caret right before the break is still in it)
            let end = text[i..].find('\n').map(|j| i + j).unwrap_or(n);
            if stop <= end {
                s.at_end = At::Comment;
                break;
            }
            i = end;
            continue;
        }
        if c == b'/' && d == b'*' {
            // MySQL /*! … */ and MariaDB /*M! … */ are executed code: read their inside as code
            if kind == DbKind::Mysql && i + 2 < n && (b[i + 2] == b'!' || (b[i + 2] == b'M' && i + 3 < n && b[i + 3] == b'!')) {
                i += if b[i + 2] == b'!' { 3 } else { 4 };
                continue;
            }
            // PostgreSQL comments nest
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                if b[j] == b'/' && j + 1 < n && b[j + 1] == b'*' && kind == DbKind::Postgres {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            if inside(&mut s, i, 2, j, depth == 0, At::Comment) {
                break;
            }
            i = j;
            continue;
        }
        let bracket_quote = c == b'[' && bracket_names(kind);
        if c == b'\'' || c == b'"' || c == b'`' || bracket_quote {
            let close = if bracket_quote { b']' } else { c };
            let escapes = (backslash && escapes_in(kind, c)) || (e_strings(kind) && c == b'\'' && is_e_string(b, i));
            let (mut j, mut closed) = (i + 1, false);
            while j < n {
                if escapes && b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == close {
                    // A doubled quote is one quote inside
                    if j + 1 < n && b[j + 1] == close {
                        j += 2;
                        continue;
                    }
                    closed = true;
                    break;
                }
                j += 1;
            }
            if inside(&mut s, i, 1, j + 1, closed, At::Quote(c)) {
                break;
            }
            i = j + 1;
            continue;
        }
        if c == b'$'
            && dollar_quotes(kind)
            && dollar_opens(b, i)
            && let Some(te) = text[i + 1..].find('$').map(|j| j + 2)
            && is_dollar_tag(&text[i..i + te])
        {
            let tag = &text[i..i + te];
            let found = text[i + te..].find(tag);
            let end = found.map(|j| i + te + j + te).unwrap_or(n);
            if inside(&mut s, i, te, end, found.is_some(), At::Dollar) {
                break;
            }
            i = end;
            continue;
        }
        match c {
            b'(' | b'[' => stack.push((i, c)),
            b')' | b']' => match stack.last() {
                Some(&(_, o)) if (o, c) == (b'(', b')') || (o, c) == (b'[', b']') => {
                    stack.pop();
                }
                _ => s.strays.push((i, c)),
            },
            b';' => s.open.append(&mut stack),
            _ => {}
        }
        i += 1;
    }
    s.open.append(&mut stack);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    const PG: DbKind = DbKind::Postgres;

    /// Types `ch` at the `|` of `text` (or over the `[…]` selection) and shows the result with the caret as `|`; None: inserted as usual
    fn ty(text: &str, ch: char, kind: DbKind) -> Option<String> {
        let (t, sel) = if let Some(a) = text.find('«') {
            let b = text.find('»').unwrap();
            (format!("{}{}{}", &text[..a], &text[a + 2..b], &text[b + 2..]), (a, b - 2))
        } else {
            let a = text.find('|').unwrap();
            (text.replacen('|', "", 1), (a, a))
        };
        typed(&t, sel, ch, kind).map(|e| show(&t, e))
    }

    fn show(t: &str, e: Edit) -> String {
        let new = format!("{}{}{}", &t[..e.start], e.insert, &t[e.end..]);
        if e.sel.0 == e.sel.1 {
            format!("{}|{}", &new[..e.sel.1], &new[e.sel.1..])
        } else {
            format!("{}«{}»{}", &new[..e.sel.0], &new[e.sel.0..e.sel.1], &new[e.sel.1..])
        }
    }

    fn bs(text: &str, kind: DbKind) -> Option<String> {
        let a = text.find('|').unwrap();
        let t = text.replacen('|', "", 1);
        backspace(&t, a, kind).map(|e| show(&t, e))
    }

    #[test]
    fn opening_chars_close_themselves() {
        assert_eq!(ty("|", '(', PG).as_deref(), Some("(|)"));
        assert_eq!(ty("where id in |", '(', PG).as_deref(), Some("where id in (|)"));
        assert_eq!(ty("a = |", '\'', PG).as_deref(), Some("a = '|'"));
        assert_eq!(ty("|", '"', PG).as_deref(), Some("\"|\""));
        assert_eq!(ty("tags|", '[', PG).as_deref(), Some("tags[|]"));
        // Before a closing bracket, a comma, ; or a space, as in Monaco
        assert_eq!(ty("f(|)", '(', PG).as_deref(), Some("f((|))"));
        assert_eq!(ty("|, b", '\'', PG).as_deref(), Some("'|', b"));
        assert_eq!(ty("x = |;", '\'', PG).as_deref(), Some("x = '|';"));
    }

    #[test]
    fn no_close_before_a_word_or_a_quote_after_a_word() {
        assert_eq!(ty("|abc", '(', PG), None);
        assert_eq!(ty("|abc", '\'', PG), None);
        // A quote right after a word (it's or E'…') stays alone, a bracket doesn't
        assert_eq!(ty("it|", '\'', PG), None);
        assert_eq!(ty("count|", '(', PG).as_deref(), Some("count(|)"));
        // Other characters insert as usual
        assert_eq!(ty("|", 'x', PG), None);
        assert_eq!(ty("|", '{', PG), None);
    }

    #[test]
    fn no_close_inside_strings_and_comments() {
        assert_eq!(ty("'abc |", '(', PG), None);
        assert_eq!(ty("'a | b'", '(', PG), None);
        assert_eq!(ty("\"name |\"", '[', PG), None);
        assert_eq!(ty("-- note |", '(', PG), None);
        assert_eq!(ty("-- note |\nselect", '\'', PG), None);
        assert_eq!(ty("/* a | */", '\'', PG), None);
        assert_eq!(ty("$$ body | $$", '(', PG), None);
        assert_eq!(ty("$f$ x |", '\'', PG), None);
        // MySQL # comments; in PostgreSQL # is an operator
        assert_eq!(ty("# note |", '(', DbKind::Mysql), None);
        assert_eq!(ty("a # |", '(', PG).as_deref(), Some("a # (|)"));
        // After a closed string or comment it closes again
        assert_eq!(ty("'a' |", '(', PG).as_deref(), Some("'a' (|)"));
        assert_eq!(ty("-- c\n|", '(', PG).as_deref(), Some("-- c\n(|)"));
        assert_eq!(ty("/* c */ |", '(', PG).as_deref(), Some("/* c */ (|)"));
    }

    #[test]
    fn closing_char_types_over() {
        assert_eq!(ty("(|)", ')', PG).as_deref(), Some("()|"));
        assert_eq!(ty("a[1|]", ']', PG).as_deref(), Some("a[1]|"));
        assert_eq!(ty("'abc|'", '\'', PG).as_deref(), Some("'abc'|"));
        assert_eq!(ty("\"|\"", '"', PG).as_deref(), Some("\"\"|"));
        // Not the opening quote of the next string, and not a bracket inside a string
        assert_eq!(ty("x |'a'", '\'', PG), None);
        assert_eq!(ty("'(|)'", ')', PG), None);
        // A closing bracket with something else after it inserts as usual
        assert_eq!(ty("(a|", ')', PG), None);
    }

    #[test]
    fn backspace_in_an_empty_pair_deletes_both() {
        assert_eq!(bs("(|)", PG).as_deref(), Some("|"));
        assert_eq!(bs("x = '|'", PG).as_deref(), Some("x = |"));
        assert_eq!(bs("[|]", PG).as_deref(), Some("|"));
        assert_eq!(bs("\"|\" x", PG).as_deref(), Some("| x"));
        // Not a pair: something between, other characters, or the doubled quote inside 'it''s'
        assert_eq!(bs("(a|)", PG), None);
        assert_eq!(bs("a|", PG), None);
        assert_eq!(bs("|", PG), None);
        assert_eq!(bs("'it'|'s'", PG), None);
        assert_eq!(bs("-- (|)", PG), None);
    }

    #[test]
    fn typing_an_opener_over_a_selection_wraps_it() {
        assert_eq!(ty("select «a + b» * 2", '(', PG).as_deref(), Some("select («a + b») * 2"));
        assert_eq!(ty("«hello»", '\'', PG).as_deref(), Some("'«hello»'"));
        assert_eq!(ty("«Name»", '"', PG).as_deref(), Some("\"«Name»\""));
        assert_eq!(ty("«1»", '[', PG).as_deref(), Some("[«1»]"));
        // Another character replaces the selection as usual
        assert_eq!(ty("«a»", ')', PG), None);
        assert_eq!(ty("«a»", 'x', PG), None);
    }

    #[test]
    fn non_ascii_offsets() {
        assert_eq!(ty("이름 = |", '\'', PG).as_deref(), Some("이름 = '|'"));
        assert_eq!(bs("'한|'", PG), None);
        assert_eq!(ty("«한글»", '(', PG).as_deref(), Some("(«한글»)"));
    }

    fn marks(text: &str, kind: DbKind) -> Vec<(String, String)> {
        unbalanced(text, kind).into_iter().map(|u| (text[u.start..u.end].to_string(), u.message)).collect()
    }

    #[test]
    fn unbalanced_brackets() {
        assert!(marks("select f(a, (b)), x[1] from t", PG).is_empty());
        assert_eq!(marks("select count(* from t", PG), [("(".into(), "Unclosed '('".into())]);
        assert_eq!(marks("select a) from t", PG), [(")".into(), "')' closes nothing".into())]);
        assert_eq!(marks("select a[1 from t", PG), [("[".into(), "Unclosed '['".into())]);
        // (] : the ] closes nothing, the ( stays open
        assert_eq!(marks("(]", PG).iter().map(|m| m.1.as_str()).collect::<Vec<_>>(), ["Unclosed '('", "']' closes nothing"]);
        // Within a statement: the ( of the first doesn't pair with the ) of the second
        let u = unbalanced("select (1;\nselect 2)", PG);
        assert_eq!(u.iter().map(|u| u.start).collect::<Vec<_>>(), [7, 19]);
    }

    #[test]
    fn unbalanced_quotes() {
        assert_eq!(marks("select 'abc from t", PG), [("'".into(), "Unclosed string: no closing '".into())]);
        assert_eq!(marks("select \"Name from t", PG), [("\"".into(), "Unclosed name: no closing \"".into())]);
        assert_eq!(marks("select `a from t", DbKind::Mysql), [("`".into(), "Unclosed name: no closing `".into())]);
        assert!(marks("select 'it''s', \"a\"\"b\"", PG).is_empty());
        // Backslash escapes in MySQL strings and PostgreSQL E'…', not in PostgreSQL '…'
        assert!(marks(r"select 'a\'b'", DbKind::Mysql).is_empty());
        assert!(marks(r"select E'a\'b'", PG).is_empty());
        assert_eq!(marks(r"select 'a\'b'", PG).len(), 1);
        // Brackets inside a string count for nothing; an open string hides the rest
        assert!(marks("select ')(' as x", PG).is_empty());
        assert_eq!(marks("select ( 'a", PG).iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["(", "'"]);
        // [name] in SQL Server and SQLite
        assert!(marks("select [a(b] from t", DbKind::Mssql).is_empty());
        assert_eq!(marks("select [a from t", DbKind::Sqlite), [("[".into(), "Unclosed name: no closing ]".into())]);
    }

    #[test]
    fn unbalanced_dollar_quotes_and_comments() {
        assert!(marks("do $$ begin perform f(; end $$;", PG).is_empty());
        assert!(marks("create function f() returns int as $body$ select (1 $body$ language sql", PG).is_empty());
        assert_eq!(marks("do $body$ begin", PG), [("$body$".into(), "Unclosed $body$ quote".into())]);
        // x$y$ is one identifier, not a quote
        assert!(marks("select x$y$ from t", PG).is_empty());
        // MySQL has no dollar quotes
        assert!(marks("select '$$' from t", DbKind::Mysql).is_empty());
        assert!(marks("select 1 -- it's (\n", PG).is_empty());
        assert!(marks("select 1 /* it's ( /* nested ) */ ' */", PG).is_empty());
        assert!(marks("select 1 # it's (", DbKind::Mysql).is_empty());
        // MySQL: --x is not a comment
        assert_eq!(marks("select 1 --'x", DbKind::Mysql).len(), 1);
        // /*! … */ is code in MySQL
        assert_eq!(marks("select /*! ( */ 1", DbKind::Mysql).len(), 1);
        // An open comment hides the rest without a mark
        assert!(marks("select ( /* (", PG).iter().map(|m| m.0.as_str()).eq(["("]));
        // Not SQL
        assert!(marks("GET 'a", DbKind::Redis).is_empty());
    }
}
