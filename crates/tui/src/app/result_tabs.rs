//! A console's result tabs follow its statements (#151): one tab per statement, kept while the statement is in the editor, at most
//! `cap` unpinned per console (⌃k › Result Tabs…). The same rules as the GUI's src/lib/resultTabs.ts (a small duplicate by design)
use crate::grid::Grid;

pub struct ResultTab {
    pub id: usize,
    /// The statement's place in the script (0-based) the tab follows; None once it stopped following (pinned, or the run wasn't one
    /// of the script's statements)
    pub stmt: Option<usize>,
    /// The number in its label ("2: customers"): its statement's place + 1, kept as it was once the tab stops following
    pub num: usize,
    pub title: String,
    /// The statement it shows the result of (run again after its edits are submitted)
    pub sql: String,
    pub grid: Grid,
    pub pinned: bool,
    /// When it last ran or was shown: past the cap the least recently shown unpinned tab closes
    pub used: u64,
}

impl ResultTab {
    /// Its label in the strip: "2: customers", 📌 in front when pinned
    pub fn label(&self) -> String {
        format!("{}{}: {}", if self.pinned { "📌 " } else { "" }, self.num, self.title)
    }
}

/// A console's result tabs and the one shown (None: Output)
#[derive(Default)]
pub struct Results {
    pub tabs: Vec<ResultTab>,
    pub shown: Option<usize>,
    /// The run about to start or in progress: its statements, their places followed through edits (edited) until their results come
    pub pending: Vec<RunAt>,
    seq: usize,
}

/// A statement to run: its place in the script (None: not one of its statements, e.g. part of one selected), its text, and the tab
/// it runs again for (submitted edits): its result goes there in place
#[derive(Clone, Debug, PartialEq)]
pub struct RunAt {
    pub stmt: Option<usize>,
    pub sql: String,
    pub into: Option<usize>,
}

/// A result tab's title: the statement's leading comment, else its table, else "Result n" (the GUI's resultTitle)
pub fn title(sql: &str, n: usize) -> String {
    static COMMENT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^\s*(?:--\s*(.+)|/\*\s*((?s).*?)\*/)").unwrap());
    static FROM: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"(?i)\bfrom\s+((?:"[^"]+"|\w+)(?:\.(?:"[^"]+"|\w+))?)"#).unwrap());
    if let Some(c) = COMMENT.captures(sql) {
        let t = c.get(1).or(c.get(2)).map_or("", |m| m.as_str()).trim();
        return t.lines().next().unwrap_or("").chars().take(40).collect();
    }
    match FROM.captures(sql) {
        Some(f) => f[1].replace('"', ""),
        None => format!("Result {n}"),
    }
}

/// The changed middle of an edit: how many statements at the start (p) and at the end (q) have the same texts before and after
fn unchanged(before: &[String], after: &[String]) -> (usize, usize) {
    let p = before.iter().zip(after).take_while(|(a, b)| a == b).count();
    let q = before[p..].iter().rev().zip(after[p..].iter().rev()).take_while(|(a, b)| a == b).count();
    (p, q)
}

/// Map old statement places to new ones after an edit, by their texts: the same leading and trailing statements keep their places; in
/// the changed middle, statements are paired in order (an edited statement keeps its tab) and the old ones left over are gone (None),
/// unless one of the middle's statements now holds its text (two merged while a statement is typed above another without its `;` yet)
pub fn remap(before: &[String], after: &[String]) -> Vec<Option<usize>> {
    let (p, q) = unchanged(before, after);
    let mid = after.len() - p - q;
    let into = |i: usize| {
        let t = before[i].trim();
        (p..after.len() - q).rev().find(|&k| !t.is_empty() && after[k].contains(t))
    };
    (0..before.len())
        .map(|i| match i {
            _ if i < p => Some(i),
            _ if i >= before.len() - q => Some(i + after.len() - before.len()),
            _ if i - p < mid => Some(i),
            _ => into(i),
        })
        .collect()
}

/// A statement's text as it is compared with what a tab ran (without the closing semicolon)
pub fn bare(sql: &str) -> &str {
    let t = sql.trim();
    t.strip_suffix(';').unwrap_or(t).trim_end()
}

impl Results {
    /// Newer than any tab's last run or show
    fn tick(&self) -> u64 {
        self.tabs.iter().map(|t| t.used).max().unwrap_or(0) + 1
    }

    pub fn shown_tab(&self) -> Option<&ResultTab> {
        self.tabs.iter().find(|t| Some(t.id) == self.shown)
    }

    pub fn shown_mut(&mut self) -> Option<&mut ResultTab> {
        self.tabs.iter_mut().find(|t| Some(t.id) == self.shown)
    }

    /// The shown tab's grid (None while Output is shown)
    pub fn grid(&self) -> Option<&Grid> {
        self.shown_tab().map(|t| &t.grid)
    }

    pub fn grid_mut(&mut self) -> Option<&mut Grid> {
        self.shown_mut().map(|t| &mut t.grid)
    }

    /// The statement's unpinned tab, else its pinned one
    pub fn tab_of(&self, stmt: usize) -> Option<&ResultTab> {
        let of = |t: &&ResultTab| t.stmt == Some(stmt);
        self.tabs.iter().filter(of).find(|t| !t.pinned).or_else(|| self.tabs.iter().find(of))
    }

    /// Statement `stmt` (None: a run that isn't one of the script's statements) returned `grid`: tab `into` (a rerun of it, pinned or
    /// not) or else the statement's unpinned tab gets it and keeps its id; none (or only a pinned one): a new tab, bound to the
    /// statement. Shown either way; the tab's id
    pub fn ran(&mut self, cap: usize, stmt: Option<usize>, into: Option<usize>, sql: String, title: String, grid: Grid) -> usize {
        let used = self.tick();
        let own = self.tabs.iter().position(|t| into == Some(t.id)).or_else(|| self.tabs.iter().position(|t| stmt.is_some() && t.stmt == stmt && !t.pinned));
        if let Some(t) = own.map(|i| &mut self.tabs[i]) {
            (t.sql, t.title, t.grid, t.used) = (sql, title, grid, used);
            self.shown = Some(t.id);
            return t.id;
        }
        // A pinned tab of this statement stays as it is and no longer follows the statement: the new tab does
        for t in self.tabs.iter_mut().filter(|t| stmt.is_some() && t.stmt == stmt) {
            t.stmt = None;
        }
        let num = stmt.map_or_else(|| self.tabs.iter().map(|t| t.num).max().unwrap_or(0) + 1, |s| s + 1);
        self.seq += 1;
        let id = self.seq;
        self.tabs.push(ResultTab { id, stmt, num, title, sql, grid, pinned: false, used });
        self.shown = Some(id);
        self.prune(cap);
        id
    }

    /// Over the cap, close the least recently shown unpinned tabs (never the shown one, nor one with edits not submitted yet); pinned
    /// tabs don't count
    pub fn prune(&mut self, cap: usize) {
        while self.tabs.iter().filter(|t| !t.pinned).count() > cap.max(1) {
            let Some(lru) = self.tabs.iter().filter(|t| !t.pinned && Some(t.id) != self.shown && t.grid.pending() == 0).min_by_key(|t| t.used).map(|t| t.id) else { return };
            self.tabs.retain(|t| t.id != lru);
        }
    }

    /// Show a tab (None: Output)
    pub fn show(&mut self, id: Option<usize>) {
        let used = self.tick();
        if let Some(t) = self.tabs.iter_mut().find(|t| Some(t.id) == id) {
            t.used = used;
        }
        self.shown = id;
    }

    /// The caret moved into statement `stmt`: its tab is shown; a statement without one (or None) leaves things as they are.
    /// Whether the shown tab changed
    pub fn caret(&mut self, stmt: Option<usize>) -> bool {
        match stmt.and_then(|s| self.tab_of(s)).map(|t| t.id) {
            Some(id) if self.shown != Some(id) => {
                self.show(Some(id));
                true
            }
            _ => false,
        }
    }

    /// The script was edited: tabs follow their statements (remap); a statement that is gone closes its unpinned tab (a pinned one
    /// stays, its number frozen, and so does one with edits not submitted yet). A tab whose statement now reads differently goes to the
    /// statement that reads exactly as it ran, if one has no tab yet: in the changed middle (a statement typed above it, split off on
    /// its `;`), anywhere once its own is gone (the first of two identical statements deleted). A tab left on a statement that another
    /// tab ran exactly as it reads now closes (its own was merged into that one, then deleted). The run in progress follows the same way
    pub fn edited(&mut self, before: &[String], after: &[String]) {
        let map = remap(before, after);
        let (p, q) = unchanged(before, after);
        let target = |s: Option<usize>| s.and_then(|s| map.get(s).copied().flatten());
        let mut taken: Vec<Option<usize>> = self.tabs.iter().map(|t| target(t.stmt)).collect();
        // Where what ran `sql` as statement `stmt` goes; `taken`: the statements with a tab (one per statement), None for a statement
        // still running (it takes none)
        let follow = |stmt: Option<usize>, sql: &str, taken: Option<&mut Vec<Option<usize>>>| {
            let to = target(stmt);
            if stmt.is_none() || to.is_some_and(|k| bare(&after[k]) == bare(sql)) {
                return to;
            }
            let free = |k: usize| (to.is_none() || (p..after.len() - q).contains(&k)) && !taken.as_ref().is_some_and(|t| t.contains(&Some(k)));
            let Some(j) = (0..after.len()).find(|&k| free(k) && bare(&after[k]) == bare(sql)) else { return to };
            if let Some(t) = taken {
                t.push(Some(j));
            }
            Some(j)
        };
        let mut to: Vec<Option<usize>> = self.tabs.iter().map(|t| follow(t.stmt, &t.sql, Some(&mut taken))).collect();
        let exact: Vec<usize> = self.tabs.iter().zip(&to).filter_map(|(t, &k)| k.filter(|&k| bare(&after[k]) == bare(&t.sql))).collect();
        for (t, k) in self.tabs.iter().zip(to.iter_mut()) {
            if k.is_some_and(|k| exact.contains(&k) && bare(&after[k]) != bare(&t.sql)) {
                *k = None;
            }
        }
        let mut keep = vec![];
        for (mut t, to) in std::mem::take(&mut self.tabs).into_iter().zip(to) {
            if t.stmt.is_some() && to.is_none() && !t.pinned && t.grid.pending() == 0 {
                continue;
            }
            if let Some(s) = to {
                t.num = s + 1;
            }
            t.stmt = to;
            keep.push(t);
        }
        self.tabs = keep;
        for r in &mut self.pending {
            r.stmt = follow(r.stmt, &r.sql, None);
        }
        if self.shown.is_some() && self.shown_tab().is_none() {
            self.shown = None;
        }
    }

    /// Close a tab; the shown one closed: Output
    pub fn close(&mut self, id: usize) {
        self.tabs.retain(|t| t.id != id);
        if self.shown == Some(id) {
            self.shown = None;
        }
    }

    /// ⌥. / ⌥,: the next or previous tab in the strip, Output first
    pub fn cycle(&mut self, d: isize) {
        let all: Vec<Option<usize>> = std::iter::once(None).chain(self.tabs.iter().map(|t| Some(t.id))).collect();
        let at = all.iter().position(|&s| s == self.shown).unwrap_or(0) as isize;
        self.show(all[(at + d).rem_euclid(all.len() as isize) as usize]);
    }

    /// ⌥o: Output, or back to the tab shown last
    pub fn toggle_output(&mut self) {
        let id = match self.shown {
            Some(_) => None,
            None => self.tabs.iter().max_by_key(|t| t.used).map(|t| t.id),
        };
        self.show(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qurry_core::db::{ColumnInfo, QueryResult};
    use qurry_core::sql;
    use qurry_core::store::DbKind;

    fn grid(n: usize) -> Grid {
        Grid::new(QueryResult {
            columns: vec![ColumnInfo { name: "id".into(), type_name: "INT8".into() }],
            rows: (0..n).map(|i| vec![serde_json::json!(i + 1)]).collect(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 1,
            has_result_set: true,
            in_transaction: false,
            warning: None,
            editable: None,
        })
    }
    const S3: [&str; 3] = ["select * from customers", "select * from orders where status = 'paid'", "select * from payments limit 5"];
    fn s3() -> Vec<String> {
        S3.iter().map(|s| s.to_string()).collect()
    }
    fn run(r: &mut Results, cap: usize, stmt: Option<usize>, title: &str, n: usize) -> usize {
        r.ran(cap, stmt, None, format!("select * from {title}"), title.into(), grid(n))
    }
    fn labels(r: &Results) -> Vec<String> {
        r.tabs.iter().map(|t| t.label()).collect()
    }
    fn two_runs() -> Results {
        let mut r = Results::default();
        run(&mut r, 10, Some(0), "customers", 8);
        run(&mut r, 10, Some(1), "orders", 8);
        r
    }

    #[test]
    fn run_1_then_2() {
        let r = two_runs();
        assert_eq!(r.tabs.iter().map(|t| t.stmt).collect::<Vec<_>>(), [Some(0), Some(1)]);
        assert_eq!(labels(&r), ["1: customers", "2: orders"]);
        assert_eq!(r.shown, Some(r.tabs[1].id));
    }

    #[test]
    fn caret_back_on_1() {
        let mut r = two_runs();
        assert!(r.caret(Some(0)));
        assert_eq!(r.shown, Some(r.tabs[0].id));
        assert!(!r.caret(Some(0)), "nothing to change");
        assert!(!r.caret(Some(2)), "a statement without a tab");
        assert!(!r.caret(None));
        assert!(r.tabs[0].used > r.tabs[1].used);
    }

    #[test]
    fn edit_1_and_run_again_keeps_its_tab() {
        let mut r = two_runs();
        let mut after = s3();
        after[0] = "select id, name from customers where city = 'Busan'".into();
        r.edited(&s3(), &after);
        let id = r.tabs[0].id;
        let to = r.ran(10, Some(0), None, after[0].clone(), "customers".into(), grid(3));
        assert_eq!(to, id);
        assert_eq!(r.tabs.len(), 2);
        assert_eq!(r.tabs[0].grid.rows(), 3);
        assert_eq!(r.tabs[0].sql, after[0]);
        assert_eq!(r.shown, Some(id));
    }

    #[test]
    fn pinned_tab_run_again_opens_a_new_tab() {
        let mut r = Results::default();
        run(&mut r, 10, Some(0), "customers", 8);
        r.tabs[0].pinned = true;
        run(&mut r, 10, Some(0), "customers", 3);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.pinned)).collect::<Vec<_>>(), [(None, true), (Some(0), false)]);
        assert_eq!(labels(&r), ["📌 1: customers", "1: customers"]);
        assert_eq!(r.tabs[0].grid.rows(), 8);
        assert_eq!(r.tab_of(0).unwrap().id, r.tabs[1].id);
    }

    #[test]
    fn statement_deleted_closes_its_tab_and_renumbers() {
        let mut r = two_runs();
        run(&mut r, 10, Some(2), "payments", 5);
        let after = vec![s3()[0].clone(), s3()[2].clone()];
        r.show(Some(r.tabs[1].id));
        r.edited(&s3(), &after);
        assert_eq!(labels(&r), ["1: customers", "2: payments"]);
        assert_eq!(r.shown, None, "the shown tab closed: Output");
        // Pinned: it survives, no longer follows, keeps 2
        let mut p = two_runs();
        run(&mut p, 10, Some(2), "payments", 5);
        p.tabs[1].pinned = true;
        p.edited(&s3(), &after);
        assert_eq!(p.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(Some(0), 1), (None, 2), (Some(1), 2)]);
        p.edited(&after, &[after[1].clone()]);
        assert_eq!(p.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(None, 2), (Some(0), 1)]);
    }

    #[test]
    fn statement_inserted_above_moves_the_tabs_down() {
        let mut r = two_runs();
        let after: Vec<String> = std::iter::once("select 1".to_string()).chain(s3()).collect();
        r.edited(&s3(), &after);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(Some(1), 2), (Some(2), 3)]);
    }

    #[test]
    fn statement_typed_right_above_another_keeps_the_tab_on_the_one_it_ran() {
        let mut r = Results::default();
        run(&mut r, 10, Some(0), "customers", 1);
        r.ran(10, Some(1), None, S3[2].into(), "payments".into(), grid(5));
        let merged = vec![S3[0].to_string(), format!("select * from contracts\n{};", S3[2])];
        r.edited(&[S3[0].to_string(), format!("{};", S3[2])], &merged);
        assert_eq!(r.tabs[1].stmt, Some(1));
        r.edited(&merged, &[S3[0].to_string(), "select * from contracts;".into(), format!("{};", S3[2])]);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(Some(0), 1), (Some(2), 3)]);
    }

    #[test]
    fn remap_cases() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(remap(&v(&["a", "b", "c"]), &v(&["a", "b", "c"])), [Some(0), Some(1), Some(2)]);
        assert_eq!(remap(&v(&["a", "b", "c"]), &v(&["a", "B", "c"])), [Some(0), Some(1), Some(2)]);
        assert_eq!(remap(&v(&["a", "b", "c"]), &v(&["a", "c"])), [Some(0), None, Some(1)]);
        assert_eq!(remap(&v(&["a", "b", "c"]), &v(&["x", "a", "b", "c"])), [Some(1), Some(2), Some(3)]);
        assert_eq!(remap(&v(&["a", "b", "c", "d"]), &v(&["a", "X", "d"])), [Some(0), Some(1), None, Some(2)]);
        assert_eq!(remap(&v(&["a"]), &[]), [None]);
        assert_eq!(remap(&v(&["a", "select", "select 2"]), &v(&["a", "select *\nselect 2"])), [Some(0), Some(1), Some(1)]);
    }

    #[test]
    fn eleventh_tab_closes_the_least_recently_shown() {
        let mut r = Results::default();
        for i in 0..10 {
            run(&mut r, 10, Some(i), &format!("t{i}"), 1);
        }
        r.caret(Some(0));
        run(&mut r, 10, Some(10), "t10", 1);
        let titles: Vec<&str> = r.tabs.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["t0", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10"]);
        assert_eq!(r.shown, Some(r.tabs[9].id));
        // Pinned tabs don't count; the shown one never closes
        let mut c = Results::default();
        run(&mut c, 2, Some(0), "a", 1);
        run(&mut c, 2, Some(1), "b", 1);
        c.tabs[0].pinned = true;
        run(&mut c, 2, Some(2), "c", 1);
        run(&mut c, 2, Some(3), "d", 1);
        assert_eq!(c.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["a", "c", "d"]);
        c.show(Some(c.tabs[1].id));
        c.prune(1);
        assert_eq!(c.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["a", "c"]);
    }

    #[test]
    fn a_run_that_is_no_script_statement_gets_its_own_tab() {
        let mut r = two_runs();
        run(&mut r, 10, None, "orders", 1);
        assert_eq!(r.tabs.iter().map(|t| t.stmt).collect::<Vec<_>>(), [Some(0), Some(1), None]);
        assert_eq!(r.tabs[2].label(), "3: orders");
        run(&mut r, 10, None, "orders", 1);
        assert_eq!(r.tabs.len(), 4);
        r.show(Some(r.tabs[0].id));
        r.caret(Some(1));
        assert_eq!(r.shown, Some(r.tabs[1].id));
    }

    #[test]
    fn error_keeps_the_statements_tab() {
        let mut r = two_runs();
        let mut after = s3();
        after[1] = "select * from order where status = 'paid'".into();
        r.edited(&s3(), &after);
        r.show(None);
        assert_eq!((r.tabs[1].stmt, r.tabs[1].title.as_str(), r.tabs[1].grid.rows()), (Some(1), "orders", 8));
        assert!(r.caret(Some(1)));
    }

    #[test]
    fn cycle_and_toggle_output() {
        let mut r = two_runs();
        let (a, b) = (r.tabs[0].id, r.tabs[1].id);
        r.cycle(1);
        assert_eq!(r.shown, None);
        r.cycle(1);
        assert_eq!(r.shown, Some(a));
        r.cycle(-1);
        r.cycle(-1);
        assert_eq!(r.shown, Some(b));
        r.toggle_output();
        assert_eq!(r.shown, None);
        r.toggle_output();
        assert_eq!(r.shown, Some(b));
    }

    #[test]
    fn a_rerun_of_a_tab_goes_into_it_in_place() {
        let mut r = two_runs();
        let sel = run(&mut r, 10, None, "users", 1);
        let to = r.ran(10, None, Some(sel), "select * from users".into(), "users".into(), grid(4));
        assert_eq!(to, sel);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.grid.rows())).collect::<Vec<_>>(), [(Some(0), 8), (Some(1), 8), (None, 4)]);
        // Pinned: in place too, still pinned and following
        r.tabs[0].pinned = true;
        let a = r.tabs[0].id;
        assert_eq!(r.ran(10, Some(0), Some(a), S3[0].into(), "customers".into(), grid(2)), a);
        assert_eq!((r.tabs.len(), r.tabs[0].stmt, r.tabs[0].pinned, r.tabs[0].grid.rows()), (3, Some(0), true, 2));
    }

    #[test]
    fn the_run_in_progress_follows_edits() {
        let mut r = two_runs();
        let two = vec![S3[0].to_string(), S3[1].to_string()];
        r.pending = vec![RunAt { stmt: Some(1), sql: S3[1].into(), into: None }];
        let after: Vec<String> = std::iter::once("select 0".to_string()).chain(two.clone()).collect();
        r.edited(&two, &after);
        assert_eq!(r.pending[0].stmt, Some(2));
        r.ran(10, r.pending[0].stmt, None, S3[1].into(), "orders".into(), grid(3));
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.title.as_str(), t.grid.rows())).collect::<Vec<_>>(), [(Some(1), "customers", 8), (Some(2), "orders", 3)]);
        // Its statement deleted: a tab of its own
        let mut d = two_runs();
        d.pending = vec![RunAt { stmt: Some(1), sql: S3[1].into(), into: None }];
        d.edited(&two, &two[..1]);
        assert_eq!(d.pending[0].stmt, None);
    }

    /// Statement texts as ⌃↩ splits them
    fn texts(t: &str) -> Vec<String> {
        sql::split_script(t, DbKind::Postgres).into_iter().map(|(s, e)| t[s..e].trim().to_string()).collect()
    }

    #[test]
    fn a_statement_backspaced_away_leaves_only_the_next_ones_tab() {
        let mut r = Results::default();
        r.ran(10, Some(0), None, "select * from users".into(), "users".into(), grid(1));
        r.ran(10, Some(1), None, "select * from payments".into(), "payments".into(), grid(1));
        let b = r.tabs[1].id;
        let mut cur = "select * from users;\nselect * from payments;".to_string();
        for at in (1..="select * from users;\n".len()).rev() {
            let next = format!("{}{}", &cur[..at - 1], &cur[at..]);
            r.edited(&texts(&cur), &texts(&next));
            cur = next;
        }
        assert_eq!(cur, "select * from payments;");
        assert_eq!(r.tabs.iter().map(|t| (t.id, t.stmt)).collect::<Vec<_>>(), [(b, Some(0))]);
    }

    #[test]
    fn merged_edited_and_split_again_one_tab_per_statement() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let mut r = Results::default();
        r.ran(10, Some(0), None, "select 1".into(), "a".into(), grid(1));
        r.ran(10, Some(1), None, "select 2".into(), "b".into(), grid(1));
        r.edited(&v(&["select 1", "select 2"]), &v(&["select 1 select 2"]));
        assert_eq!(r.tabs.iter().map(|t| t.stmt).collect::<Vec<_>>(), [Some(0), Some(0)]);
        r.edited(&v(&["select 1 select 2"]), &v(&["select 1 select 3"]));
        r.edited(&v(&["select 1 select 3"]), &v(&["select 1", "select 3"]));
        assert_eq!(r.tabs.iter().map(|t| (t.title.as_str(), t.stmt)).collect::<Vec<_>>(), [("a", Some(0))]);
    }

    #[test]
    fn the_first_of_two_identical_statements_deleted_keeps_the_others_tab() {
        let mut r = Results::default();
        run(&mut r, 10, Some(1), "t", 1);
        r.edited(&["select * from t".into(), "select * from t".into()], &["select * from t".into()]);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(Some(0), 1)]);
    }

    #[test]
    fn a_tab_with_edits_not_submitted_is_never_closed_by_an_edit_or_the_cap() {
        let mut r = two_runs();
        r.tabs[0].grid.set_value(0, 0, "9".into());
        // Its statement deleted: it stays, no longer following
        r.edited(&s3()[..2], &s3()[1..2]);
        assert_eq!(r.tabs.iter().map(|t| (t.stmt, t.num)).collect::<Vec<_>>(), [(None, 1), (Some(0), 1)]);
        // Past the cap: the least recently shown without edits closes instead
        run(&mut r, 2, Some(1), "payments", 1);
        assert_eq!(r.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["customers", "payments"]);
    }

    #[test]
    fn titles() {
        assert_eq!(title("select * from public.users", 1), "public.users");
        assert_eq!(title("-- by status\nselect * from orders", 2), "by status");
        assert_eq!(title("select 1", 3), "Result 3");
        assert_eq!(title(r#"select * from "Order""#, 1), "Order");
    }
}
