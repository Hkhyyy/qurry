//! What one statement's text names, and where (#62): its query blocks (the statement, each subquery and CTE body, each branch of a
//! UNION), the sources each block reads (tables, subqueries and CTEs by alias, USING / NATURAL joins), CTEs and select lists.
//! complete.rs reads it for completion, signature help and unknown-name marks, so all three see the same scope.
//! Only the shape is read here; what a name refers to (tables, columns) is complete.rs's, against the metadata.

/// A token's kind. Strings and comments are skipped as whole tokens, so their words never count
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    Word,
    /// `"name"` (MySQL: `` `name` ``)
    Quoted,
    Str,
    Num,
    Punct,
}

#[derive(Clone, Copy, Debug)]
pub struct Tok {
    /// Byte range in the text
    pub s: usize,
    pub e: usize,
    pub k: K,
}

/// Tokens of SQL text (comments and whitespace skipped). PostgreSQL `"…"` is a name, MySQL's is a string and `` `…` `` a name
pub fn lex(text: &str, mysql: bool) -> Vec<Tok> {
    let b = text.as_bytes();
    let n = b.len();
    let mut out = vec![];
    let mut i = 0;
    let ch = |i: usize| text[i..].chars().next().unwrap_or('\0');
    while i < n {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        // Comments: -- (MySQL: followed by a space, a control character or the end), # in MySQL, /* … */
        if c == b'-' && b.get(i + 1) == Some(&b'-') && (!mysql || b.get(i + 2).is_none_or(|x| *x <= b' ' || *x == 0x7f)) || (mysql && c == b'#') {
            i = text[i..].find('\n').map_or(n, |j| i + j);
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = text[i + 2..].find("*/").map_or(n, |j| i + 2 + j + 2);
            continue;
        }
        // \ escapes in MySQL strings and in PostgreSQL E'…'
        let quote = |q: u8, i: usize| {
            let escapes = if mysql { q != b'`' } else { q == b'\'' && crate::sql::is_e_string(b, i) };
            let mut j = i + 1;
            while j < n {
                if escapes && b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == q {
                    if b.get(j + 1) == Some(&q) {
                        j += 2;
                        continue;
                    }
                    return j + 1;
                }
                j += 1;
            }
            n
        };
        let k = match c {
            b'\'' => {
                i = quote(c, i);
                K::Str
            }
            b'"' if mysql => {
                i = quote(c, i);
                K::Str
            }
            b'"' | b'`' => {
                i = quote(c, i).min(n);
                K::Quoted
            }
            b'$' if !mysql && dollar_tag(&text[i..]).is_some() => {
                let tag = dollar_tag(&text[i..]).unwrap();
                i = text[i + tag.len()..].find(tag).map_or(n, |j| i + tag.len() + j + tag.len());
                K::Str
            }
            _ if c.is_ascii_digit() => {
                while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                    i += 1;
                }
                K::Num
            }
            _ if crate::complete::is_word(ch(i)) => {
                while i < n && (crate::complete::is_word(ch(i)) || b[i] == b'$') {
                    i += ch(i).len_utf8();
                }
                K::Word
            }
            _ => {
                i += if text[i..].starts_with("::") { 2 } else { ch(i).len_utf8() };
                K::Punct
            }
        };
        out.push(Tok { s: start, e: i, k });
    }
    out
}

/// `$$` / `$tag$` at the start of `s`
fn dollar_tag(s: &str) -> Option<&str> {
    let end = s[1..].find('$')? + 2;
    let tag = &s[..end];
    tag[1..end - 1].chars().all(|c| c.is_alphanumeric() || c == '_').then_some(tag).filter(|t| !t[1..].starts_with(|c: char| c.is_ascii_digit()))
}

/// A source a block reads: a table, a subquery or a CTE (by its alias), or something whose columns can't be read (a function)
#[derive(Clone, Debug, Default)]
pub struct Source {
    /// The table as written (`schema.table`, unquoted), for a table or a CTE reference
    pub table: Option<String>,
    pub alias: Option<String>,
    /// A subquery's block
    pub sub: Option<usize>,
    /// Column aliases: `(select …) as x(a, b)`
    pub cols: Vec<String>,
    /// Tokens of the reference (the table's name parts), for marks
    pub toks: (usize, usize),
    /// All of its tokens: the reference or subquery, its alias, its column aliases
    pub span: (usize, usize),
    /// Joined (`join t`), and whether its ON / USING is written
    pub join: bool,
    pub cond: bool,
    /// USING (columns) of a join, NATURAL
    pub using: Vec<String>,
    pub natural: bool,
    /// A function, VALUES list, parenthesized join … : its columns aren't known
    pub opaque: bool,
}

impl Source {
    /// The name the statement calls it by: the alias, else the table's name without its schema
    pub fn name(&self) -> Option<&str> {
        self.alias.as_deref().or(self.table.as_deref().map(|t| t.rsplit('.').next().unwrap_or(t)))
    }
}

#[derive(Clone, Debug)]
pub struct Cte {
    pub name: String,
    /// `with x(a, b) as (…)`
    pub cols: Vec<String>,
    pub block: Option<usize>,
}

/// An item of a select list: its tokens, its alias (AS or a trailing name), and for `*` / `x.*` the qualifier
#[derive(Clone, Debug)]
pub struct SelItem {
    pub toks: (usize, usize),
    pub alias: Option<String>,
    pub star: Option<Option<String>>,
}

/// One query block: the statement, a subquery, a CTE body or a branch of a UNION
#[derive(Clone, Debug, Default)]
pub struct Block {
    /// Byte range of its text (a subquery: inside its parentheses; an unclosed one runs to the end)
    pub span: (usize, usize),
    pub parent: Option<usize>,
    /// The first word, lowercase (select, insert, update, delete, with, values, create …)
    pub head: String,
    pub sources: Vec<Source>,
    pub ctes: Vec<Cte>,
    pub items: Vec<SelItem>,
    /// INSERT's column list: the tokens inside its parentheses
    pub insert_cols: Option<(usize, usize)>,
    /// The token range the block covers
    pub toks: (usize, usize),
    /// It can't see its parent's sources: a CTE body, a FROM subquery that isn't LATERAL (its CTEs it still sees)
    pub closed: bool,
    /// Token ranges that name no column: a SELECT … INTO target (`into temp t`, `into outfile '…'`, `into @v`)
    pub skip: Vec<(usize, usize)>,
}

pub struct Scope {
    pub text: String,
    pub toks: Vec<Tok>,
    /// For each `(`, its `)` (the end of the tokens when unclosed); for others, None
    pub close: Vec<Option<usize>>,
    pub blocks: Vec<Block>,
}

/// Words that start a clause (end a source list or a select list)
const CLAUSES: [&str; 22] = [
    "from", "where", "group", "having", "order", "limit", "offset", "union", "intersect", "except", "window", "returning", "into", "set", "values", "on", "using", "join", "fetch", "for", "select", "qualify",
];
const JOIN_WORDS: [&str; 8] = ["join", "left", "right", "full", "inner", "cross", "natural", "straight_join"];
/// MySQL's words between SELECT and its list
const SELECT_MODIFIERS: [&str; 9] = ["distinctrow", "high_priority", "straight_join", "sql_small_result", "sql_big_result", "sql_buffer_result", "sql_no_cache", "sql_cache", "sql_calc_found_rows"];

pub fn is_kw(w: &str, set: &[&str]) -> bool {
    set.iter().any(|k| w.eq_ignore_ascii_case(k))
}

impl Scope {
    pub fn new(text: &str, mysql: bool) -> Self {
        let toks = lex(text, mysql);
        let mut close = vec![None; toks.len()];
        let mut stack = vec![];
        for (i, t) in toks.iter().enumerate() {
            match &text[t.s..t.e] {
                "(" if t.k == K::Punct => stack.push(i),
                ")" if t.k == K::Punct => {
                    if let Some(o) = stack.pop() {
                        close[o] = Some(i);
                    }
                }
                _ => {}
            }
        }
        for o in stack {
            close[o] = Some(toks.len());
        }
        let mut sc = Scope { text: text.to_string(), toks, close, blocks: vec![] };
        let n = sc.toks.len();
        sc.query(0, n, None, (0, text.len()));
        sc
    }

    /// A token's text
    pub fn t(&self, i: usize) -> &str {
        self.toks.get(i).map_or("", |t| &self.text[t.s..t.e])
    }

    /// The text of tokens [a, b)
    pub fn text_of(&self, a: usize, b: usize) -> &str {
        if a >= b || b > self.toks.len() {
            return "";
        }
        &self.text[self.toks[a].s..self.toks[b - 1].e]
    }

    /// A token's name: a word as written, a quoted name without its quotes
    pub fn name(&self, i: usize) -> String {
        let s = self.t(i);
        match self.toks[i].k {
            K::Quoted => {
                let q = &s[..1];
                let inner = s.strip_prefix(q).unwrap_or(s);
                inner.strip_suffix(q).unwrap_or(inner).replace(&format!("{q}{q}"), q)
            }
            _ => s.to_string(),
        }
    }

    pub fn is(&self, i: usize, w: &str) -> bool {
        self.toks.get(i).is_some_and(|t| t.k == K::Word) && self.t(i).eq_ignore_ascii_case(w)
    }

    pub fn punct(&self, i: usize, p: &str) -> bool {
        self.toks.get(i).is_some_and(|t| t.k == K::Punct) && self.t(i) == p
    }

    fn ident(&self, i: usize) -> bool {
        self.toks.get(i).is_some_and(|t| matches!(t.k, K::Word | K::Quoted))
    }

    /// Whether the parenthesized group opening at `o` holds a query
    pub fn is_query(&self, o: usize) -> bool {
        let mut i = o + 1;
        while self.punct(i, "(") {
            i += 1;
        }
        self.is(i, "select") || self.is(i, "with") || self.is(i, "values") || self.is(i, "table")
    }

    /// The tokens [a, b) as one query (split at UNION / INTERSECT / EXCEPT into blocks of the same parent)
    fn query(&mut self, a: usize, b: usize, parent: Option<usize>, span: (usize, usize)) -> usize {
        let mut a = a;
        // `((select …))`
        if self.punct(a, "(") && self.close[a] == Some(b.saturating_sub(1)) && b > a + 1 {
            return self.query(a + 1, b - 1, parent, span);
        }
        let first = self.blocks.len();
        let mut i = a;
        let mut cut = vec![];
        while i < b {
            if self.punct(i, "(") {
                i = self.close[i].unwrap_or(b) + 1;
                continue;
            }
            if self.is(i, "union") || self.is(i, "intersect") || self.is(i, "except") {
                cut.push(i);
            }
            i += 1;
        }
        let mut ctes = vec![];
        for (k, end) in cut.iter().copied().chain([b]).enumerate() {
            let tok_span = |s: &Self, x: usize, y: usize| (s.toks.get(x).map_or(span.0, |t| t.s).max(span.0), if y >= b { span.1 } else { s.toks[y].s });
            let bs = tok_span(self, a, end);
            let idx = self.block(a, end, parent, bs);
            // A WITH before the first branch belongs to the whole query
            if k == 0 {
                ctes = self.blocks[idx].ctes.clone();
            } else {
                self.blocks[idx].ctes = ctes.clone();
            }
            a = end + 1;
            while self.is(a, "all") || self.is(a, "distinct") {
                a += 1;
            }
        }
        first
    }

    /// One block over tokens [a, b): reads its WITH, sources, select list and INSERT column list, and the queries nested in it
    fn block(&mut self, a: usize, b: usize, parent: Option<usize>, span: (usize, usize)) -> usize {
        let me = self.blocks.len();
        self.blocks.push(Block { span, parent, head: self.t(a).to_lowercase(), toks: (a, b), ..Default::default() });
        let mut i = a;
        // WITH [RECURSIVE] name [(cols)] AS [NOT] [MATERIALIZED] (query), …
        if self.is(i, "with") {
            i += 1;
            if self.is(i, "recursive") {
                i += 1;
            }
            while i < b && self.ident(i) {
                let name = self.name(i);
                i += 1;
                let mut cols = vec![];
                if self.punct(i, "(") {
                    let c = self.close[i].unwrap_or(b);
                    cols = self.names_in(i + 1, c);
                    i = c + 1;
                }
                if self.is(i, "as") {
                    i += 1;
                }
                while self.is(i, "not") || self.is(i, "materialized") {
                    i += 1;
                }
                // Visible in its own body (WITH RECURSIVE) and in every CTE after it
                self.blocks[me].ctes.push(Cte { name, cols, block: None });
                if self.punct(i, "(") {
                    let c = self.close[i].unwrap_or(b);
                    let sp = (self.toks[i].e, self.toks.get(c).map_or(self.text.len(), |t| t.s));
                    let q = self.query(i + 1, c.min(b), Some(me), sp);
                    self.close_blocks(q, me);
                    self.blocks[me].ctes.last_mut().unwrap().block = Some(q);
                    i = c + 1;
                }
                if self.punct(i, ",") {
                    i += 1;
                } else {
                    break;
                }
            }
            if i < b {
                self.blocks[me].head = self.t(i).to_lowercase();
            }
        }
        let head = self.blocks[me].head.clone();
        let body = i;
        let mut sel_start = None;
        let mut natural = false;
        while i < b {
            let w = self.t(i).to_lowercase();
            let word = self.toks[i].k == K::Word;
            if self.punct(i, "(") {
                let c = self.close[i].unwrap_or(b);
                if self.is_query(i) {
                    let sp = (self.toks[i].e, self.toks.get(c).map_or(self.text.len(), |t| t.s));
                    self.query(i + 1, c.min(b), Some(me), sp);
                } else {
                    self.nested(i + 1, c.min(b), me);
                }
                i = c + 1;
                continue;
            }
            if word && w == "select" {
                i += 1;
                while self.is(i, "distinct") || self.is(i, "all") || self.toks.get(i).is_some_and(|t| t.k == K::Word) && is_kw(self.t(i), &SELECT_MODIFIERS) {
                    i += 1;
                    if self.is(i, "on") && self.punct(i + 1, "(") {
                        i = self.close[i + 1].unwrap_or(b) + 1;
                    }
                }
                sel_start = Some(i);
                continue;
            }
            // `a is [not] distinct from b` is an operator, not a FROM clause
            let distinct_from = w == "from" && i > 0 && self.is(i - 1, "distinct");
            // A clause word ends the select list
            if word && !distinct_from && (is_kw(&w, &CLAUSES) || is_kw(&w, &JOIN_WORDS) || matches!(w.as_str(), "update" | "delete" | "group" | "order"))
                && let Some(s) = sel_start.take()
            {
                self.items(s, i, me);
            }
            // SELECT … INTO a variable, a new table or a file: no source, and its words aren't columns
            if word && w == "into" && head == "select" {
                let s = i + 1;
                i = s;
                while i < b && !(self.toks[i].k == K::Word && is_kw(self.t(i), &CLAUSES)) {
                    if self.punct(i, "(") {
                        i = self.close[i].unwrap_or(b);
                    }
                    i += 1;
                }
                self.blocks[me].skip.push((s, i.min(b)));
                continue;
            }
            let at_ref = word && match w.as_str() {
                "from" => !distinct_from,
                "join" | "straight_join" => true,
                "into" => true,
                "update" => i == body,
                // DELETE … USING t: a source list (a join's USING is a column list in parentheses)
                "using" => head == "delete" && !self.punct(i + 1, "("),
                _ => false,
            };
            if word && w == "natural" {
                natural = true;
            }
            if at_ref {
                let join = w == "join" || w == "straight_join";
                i += 1;
                loop {
                    let into = w == "into" && head == "insert";
                    i = self.source(i, b, me, join, natural, into);
                    natural = false;
                    if !join && self.punct(i, ",") {
                        i += 1;
                        continue;
                    }
                    break;
                }
                continue;
            }
            if word
                && (w == "on" || w == "using")
                && let Some(s) = self.blocks[me].sources.last_mut()
                && s.join
            {
                s.cond = true;
                if w == "using" && self.punct(i + 1, "(") {
                    let c = self.close[i + 1].unwrap_or(b);
                    let cols = self.names_in(i + 2, c);
                    self.blocks[me].sources.last_mut().unwrap().using = cols;
                    i = c + 1;
                    continue;
                }
            }
            i += 1;
        }
        if let Some(s) = sel_start {
            self.items(s, b, me);
        }
        me
    }

    /// Parentheses that aren't a query (function arguments, IN lists, expressions): the queries inside them belong to `me`
    fn nested(&mut self, a: usize, b: usize, me: usize) {
        let mut i = a;
        while i < b {
            if self.punct(i, "(") {
                let c = self.close[i].unwrap_or(b);
                if self.is_query(i) {
                    let sp = (self.toks[i].e, self.toks.get(c).map_or(self.text.len(), |t| t.s));
                    self.query(i + 1, c.min(b), Some(me), sp);
                } else {
                    self.nested(i + 1, c.min(b), me);
                }
                i = c + 1;
                continue;
            }
            i += 1;
        }
    }

    /// One source at `i` (after FROM, JOIN, `,` …); returns where it ends
    fn source(&mut self, mut i: usize, b: usize, me: usize, join: bool, natural: bool, into: bool) -> usize {
        let first = i;
        let mut s = Source { join, natural, ..Default::default() };
        let mut lateral = false;
        while self.is(i, "only") || self.is(i, "lateral") {
            lateral |= self.is(i, "lateral");
            i += 1;
        }
        if self.punct(i, "(") {
            let c = self.close[i].unwrap_or(b);
            if self.is_query(i) {
                let sp = (self.toks[i].e, self.toks.get(c).map_or(self.text.len(), |t| t.s));
                let q = self.query(i + 1, c.min(b), Some(me), sp);
                // Only a LATERAL subquery sees the sources before it
                if !lateral {
                    self.close_blocks(q, me);
                }
                s.sub = Some(q);
            } else {
                self.nested(i + 1, c.min(b), me);
                s.opaque = true;
            }
            i = c + 1;
        } else if self.ident(i) && !(self.toks[i].k == K::Word && is_kw(self.t(i), &CLAUSES)) {
            let start = i;
            let mut parts = vec![self.name(i)];
            i += 1;
            while self.punct(i, ".") && self.ident(i + 1) {
                parts.push(self.name(i + 1));
                i += 2;
            }
            s.toks = (start, i);
            s.table = Some(parts.join("."));
            // A set-returning function (`generate_series(…)`, `json_table(…)`): columns unknown. INSERT INTO t (cols): the list
            if self.punct(i, "(") && !into {
                let c = self.close[i].unwrap_or(b);
                self.nested(i + 1, c.min(b), me);
                s.opaque = true;
                i = c + 1;
            }
        } else {
            return i;
        }
        if self.is(i, "as") {
            i += 1;
        }
        if self.ident(i) && !(self.toks[i].k == K::Word && (crate::complete::is_keyword(self.t(i)) || is_kw(self.t(i), &CLAUSES) || is_kw(self.t(i), &JOIN_WORDS) || self.is(i, "default") || self.index_hint(i))) {
            s.alias = Some(self.name(i));
            i += 1;
        }
        if self.punct(i, "(") {
            let c = self.close[i].unwrap_or(b);
            if into {
                self.blocks[me].insert_cols = Some((i + 1, c.min(b)));
            } else {
                s.cols = self.names_in(i + 1, c);
            }
            i = c + 1;
        }
        // MySQL's index hints: USE | FORCE | IGNORE INDEX | KEY [FOR JOIN | ORDER BY | GROUP BY] (names), … (index names, not columns)
        while self.index_hint(i) {
            i += 2;
            if self.is(i, "for") {
                i += if self.is(i + 1, "join") { 2 } else { 3 };
            }
            if self.punct(i, "(") {
                i = self.close[i].unwrap_or(b) + 1;
            }
            if self.punct(i, ",") && self.index_hint(i + 1) {
                i += 1;
            }
        }
        s.span = (first, i.min(b));
        self.blocks[me].sources.push(s);
        i
    }

    fn index_hint(&self, i: usize) -> bool {
        (self.is(i, "use") || self.is(i, "force") || self.is(i, "ignore")) && (self.is(i + 1, "index") || self.is(i + 1, "key"))
    }

    /// The branches of the query whose first block is `first` (blocks of `parent`) can't see `parent`'s sources
    fn close_blocks(&mut self, first: usize, parent: usize) {
        for b in &mut self.blocks[first..] {
            if b.parent == Some(parent) {
                b.closed = true;
            }
        }
    }

    /// The names in a comma list (a column list)
    fn names_in(&self, a: usize, b: usize) -> Vec<String> {
        (a..b.min(self.toks.len())).filter(|&i| self.ident(i)).map(|i| self.name(i)).collect()
    }

    /// The select list's items between `a` and `b`
    fn items(&mut self, a: usize, b: usize, me: usize) {
        let mut start = a;
        let mut i = a;
        let mut out = vec![];
        while i < b {
            if self.punct(i, "(") {
                i = self.close[i].unwrap_or(b) + 1;
                continue;
            }
            if self.punct(i, ",") {
                if start < i {
                    out.push(self.item(start, i));
                }
                start = i + 1;
            }
            i += 1;
        }
        if start < b {
            out.push(self.item(start, b));
        }
        self.blocks[me].items = out;
    }

    fn item(&self, a: usize, b: usize) -> SelItem {
        let star = if b == a + 1 && self.punct(a, "*") {
            Some(None)
        } else if b >= a + 3 && self.punct(b - 1, "*") && self.punct(b - 2, ".") && self.ident(b - 3) {
            Some(Some(self.name(b - 3)))
        } else {
            None
        };
        let last = b - 1;
        // `expr AS name`, or `expr name` after a name, `)`, a string or a number (not `a.b`, `x::type`)
        let named = self.is(last - 1, "as")
            || !crate::complete::is_keyword(self.t(last)) && !self.punct(last - 1, ".") && !self.punct(last - 1, "::") && (self.ident(last - 1) && !crate::complete::is_keyword(self.t(last - 1)) || self.punct(last - 1, ")") || matches!(self.toks[last - 1].k, K::Str | K::Num));
        let alias = (b >= a + 2 && self.ident(last) && named).then(|| self.name(last));
        SelItem { toks: (a, b), alias, star }
    }

    /// The innermost block whose text holds the byte offset `at`
    pub fn block_at(&self, at: usize) -> usize {
        let mut best = 0;
        for (i, b) in self.blocks.iter().enumerate() {
            let bb = &self.blocks[best];
            if b.span.0 <= at && at <= b.span.1 && (b.span.1 - b.span.0) <= (bb.span.1 - bb.span.0) {
                best = i;
            }
        }
        best
    }

    /// The block and its enclosing ones, innermost first
    pub fn chain(&self, b: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(b), |&i| self.blocks[i].parent)
    }

    /// The block and the enclosing ones whose sources it sees (up to a CTE body or a FROM subquery), innermost first
    pub fn sees(&self, b: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(b), |&i| if self.blocks[i].closed { None } else { self.blocks[i].parent })
    }

    /// The CTE `name` (lowercase compare) visible from block `b`
    pub fn cte(&self, b: usize, name: &str) -> Option<&Cte> {
        self.chain(b).find_map(|i| self.blocks[i].ctes.iter().find(|c| c.name.eq_ignore_ascii_case(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_strings_comments_and_names() {
        let t = "select 'a -- b' , \"My T\" -- c\n from x /* y */ where $$z$$ = 1.5::int";
        let s = Scope::new(t, false);
        let words: Vec<String> = (0..s.toks.len()).map(|i| s.name(i)).collect();
        assert_eq!(words, ["select", "'a -- b'", ",", "My T", "from", "x", "where", "$$z$$", "=", "1.5", "::", "int"]);
        let m = Scope::new("select `a``b`, \"s\" # c\nfrom t", true);
        assert_eq!((m.name(1), m.toks[3].k), ("a`b".to_string(), K::Str));
        // PostgreSQL E'…': \' doesn't end it; in '…' \ is a plain character
        let e = r"select E'it\'s', 'a\', x";
        let kinds: Vec<K> = lex(e, false).iter().map(|t| t.k).collect();
        assert_eq!(kinds, [K::Word, K::Word, K::Str, K::Punct, K::Str, K::Punct, K::Word]);
    }

    #[test]
    fn reads_sources_ctes_and_subqueries() {
        let t = "with c(a, b) as (select 1, 2), d as (select x k from t1) select * from c join (select y as yy from t2 z) s on true natural join u using (id) where c.a in (select 1 from t3)";
        let s = Scope::new(t, false);
        let top = s.block_at(t.find("where").unwrap());
        let b = &s.blocks[top];
        assert_eq!(b.head, "select");
        assert_eq!(b.ctes.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["c", "d"]);
        assert_eq!(b.ctes[0].cols, ["a", "b"]);
        let names: Vec<_> = b.sources.iter().map(|x| x.name().unwrap_or("?")).collect();
        assert_eq!(names, ["c", "s", "u"]);
        assert!(b.sources[2].natural && b.sources[2].using == ["id"]);
        let sub = &s.blocks[b.sources[1].sub.unwrap()];
        assert_eq!(sub.items[0].alias.as_deref(), Some("yy"));
        assert_eq!(sub.sources[0].alias.as_deref(), Some("z"));
        let inner = s.block_at(t.find("t3").unwrap());
        assert_eq!(s.chain(inner).count(), 2);
        let d = &s.blocks[b.ctes[1].block.unwrap()];
        assert_eq!(d.items[0].alias.as_deref(), Some("k"));
    }

    #[test]
    fn reads_insert_update_delete() {
        let s = Scope::new("insert into orders (id, user_id) values (1, 2)", false);
        let b = &s.blocks[0];
        assert_eq!(b.sources[0].table.as_deref(), Some("orders"));
        let (x, y) = b.insert_cols.unwrap();
        assert_eq!(s.names_in(x, y), ["id", "user_id"]);
        let s = Scope::new("update users u set name = 'x' from orders o where o.user_id = u.id", false);
        assert_eq!(s.blocks[0].sources.iter().map(|x| x.name().unwrap()).collect::<Vec<_>>(), ["u", "o"]);
        let s = Scope::new("delete from users using orders o where o.id = 1", false);
        assert_eq!(s.blocks[0].sources.iter().map(|x| x.name().unwrap()).collect::<Vec<_>>(), ["users", "o"]);
        let s = Scope::new("select * from generate_series(1, 3) g, users", false);
        assert!(s.blocks[0].sources[0].opaque && !s.blocks[0].sources[1].opaque);
    }

    #[test]
    fn unclosed_subquery_runs_to_the_end() {
        let t = "select * from users u where u.id in (select o.";
        let s = Scope::new(t, false);
        let b = s.block_at(t.len());
        assert_ne!(b, 0);
        assert_eq!(s.blocks[b].parent, Some(0));
    }

    #[test]
    fn cte_bodies_and_from_subqueries_dont_see_outer_sources() {
        let t = "with c as (select 1 from t1) select * from t2, (select 2 from t3) d, lateral (select 3 from t4) e where exists (select 4 from t5)";
        let s = Scope::new(t, false);
        let sees = |w: &str| s.sees(s.block_at(t.find(w).unwrap())).count();
        assert_eq!([sees("t1"), sees("t3"), sees("t4"), sees("t5")], [1, 1, 2, 2]);
        // CTEs stay visible from a CTE body
        assert!(s.cte(s.block_at(t.find("t1").unwrap()), "c").is_some());
        // MySQL: index hints and SELECT … INTO targets aren't sources or aliases
        let s = Scope::new("select id into @v from users use index (primary) where id is distinct from 2", true);
        assert_eq!(s.blocks[0].sources.iter().map(|x| (x.name().unwrap(), x.alias.is_none())).collect::<Vec<_>>(), [("users", true)]);
        assert_eq!(s.blocks[0].skip.len(), 1);
    }

    #[test]
    fn union_branches_are_their_own_blocks() {
        let t = "select a from t1 union all select b from t2";
        let s = Scope::new(t, false);
        let b = s.block_at(t.len());
        assert_eq!(s.blocks[b].sources[0].table.as_deref(), Some("t2"));
        assert_eq!(s.blocks[s.block_at(10)].sources[0].table.as_deref(), Some("t1"));
    }
}
