//! Import Data Sources (⌃k): a DataGrip / IntelliJ / DBeaver location found on this computer, or a typed file or folder (also a
//! Qurry export), then the confirm with one row per source: Space leaves an added row out, y / Enter imports the checked ones.
//! The texts come from qurry_core::import, as in the GUI
use crate::app::{err, App, PAction, PItem, Palette, Popup};
use qurry_core::error::AppResult;
use qurry_core::import::{ImportPlan, ImportRow, TITLE};
use std::path::PathBuf;
use tui_textarea::TextArea;

/// The import confirm: the plan, which rows stay checked, the row under the cursor (only checkable rows take it)
pub struct ImportPick {
    pub plan: ImportPlan,
    pub checked: Vec<bool>,
    pub sel: usize,
}

impl ImportPick {
    pub fn new(plan: ImportPlan) -> Self {
        let checked: Vec<bool> = plan.rows.iter().map(|r| r.checkable).collect();
        let sel = checked.iter().position(|c| *c).unwrap_or(0);
        ImportPick { plan, checked, sel }
    }

    /// Checked rows (what y / Enter imports)
    pub fn count(&self) -> usize {
        self.checked.iter().filter(|c| **c).count()
    }

    /// To the next checkable row up or down (skipped rows have no checkbox)
    pub fn move_by(&mut self, d: isize) {
        let rows = &self.plan.rows;
        let mut i = self.sel as isize;
        for _ in 0..d.unsigned_abs() {
            let mut j = i + d.signum();
            while j >= 0 && (j as usize) < rows.len() && !rows[j as usize].checkable {
                j += d.signum();
            }
            if j < 0 || j as usize >= rows.len() {
                break;
            }
            i = j;
        }
        self.sel = i as usize;
    }

    pub fn toggle(&mut self) {
        if self.plan.rows.get(self.sel).is_some_and(|r| r.checkable) {
            self.checked[self.sel] = !self.checked[self.sel];
        }
    }
}

/// A row as one line of text (the notice when nothing can be imported): text · note · value
pub fn row_text(r: &ImportRow) -> String {
    format!("{}\n    {} · {}", r.text, r.note, r.value)
}

/// "~/x" → the home folder
fn expand(path: &str, home: &std::path::Path) -> PathBuf {
    match path.trim().strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path.trim() == "~" => home.to_path_buf(),
        None => PathBuf::from(path.trim()),
    }
}

impl App {
    /// ⌃k › Import Data Sources…: the locations found (newest IDE version first), then File or folder…
    pub fn ds_import_start(&mut self) {
        let found = qurry_core::import::locations(&self.import_home);
        let mut items: Vec<PItem> = found.iter().map(|l| PItem { label: format!("{}  ({})", l.label, l.count), hint: l.shown.clone(), action: PAction::ImportFrom(l.path.clone()) }).collect();
        let none = if found.is_empty() { "No DataGrip, IntelliJ or DBeaver settings found · " } else { "" };
        items.push(PItem { label: "File or folder…".into(), hint: format!("{none}Qurry export · dataSources.xml · .idea · data-sources.json"), action: PAction::ImportPath });
        self.popup = Some(Popup::Palette(Palette { title: "Import Data Sources from".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// File or folder…: a path (the GUI's native picker)
    pub fn ds_import_path(&mut self) {
        let body = "A Qurry export, dataSources.xml, a project or its .idea, an IDE options folder (…/JetBrains/DataGrip2025.2/options),\n\
                    DBeaver's data-sources.json, a .dbeaver or workspace folder. ~ is your home folder."
            .to_string();
        self.popup = Some(Popup::Input {
            title: "Import Data Sources from (file or folder)".into(),
            body,
            input: TextArea::default(),
            on_ok: Box::new(|a, s| {
                if s.trim().is_empty() {
                    return a.ds_import_path();
                }
                let path = expand(&s, &a.import_home);
                let r = a.state.import_plan(&path, &a.import_home);
                a.ds_import_show(r);
            }),
            filter: None,
            complete: None,
        });
    }

    /// A found location (only one `locations` lists is read)
    pub fn ds_import_location(&mut self, path: String) {
        let r = self.state.import_location(&path, &self.import_home);
        self.ds_import_show(r);
    }

    /// The confirm, or a notice: nothing importable (the rows and why) or the file couldn't be read (nothing was imported)
    fn ds_import_show(&mut self, r: AppResult<ImportPlan>) {
        match r {
            Err(e) => {
                let e = e.to_string();
                self.status = e.lines().next().unwrap_or_default().to_string();
                self.notice(TITLE.into(), e);
            }
            Ok(plan) if plan.importable == 0 => {
                self.status = plan.message.lines().next().unwrap_or_default().to_string();
                let rows: Vec<String> = plan.rows.iter().map(row_text).collect();
                self.notice(TITLE.into(), format!("{}\n\n{}", plan.message, rows.join("\n")));
            }
            Ok(plan) => self.popup = Some(Popup::ImportPick(ImportPick::new(plan))),
        }
    }

    /// y / Enter: import the checked rows
    pub fn ds_import_apply(&mut self, pick: ImportPick) {
        let st = self.state.clone();
        self.status = format!("Importing {} data source(s)…", pick.count());
        self.spawn(async move { st.import_apply(&pick.plan, &pick.checked).await.map_err(err) }, |a, r| match r {
            Ok(r) => {
                a.status = r.message;
                a.reload_conns();
            }
            Err(e) => a.notice(TITLE.into(), e),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    fn fixtures() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/import").canonicalize().unwrap()
    }

    /// Typed into the palette's filter, then Enter
    fn pick(h: &mut Harness, filter: &str) {
        h.type_str(filter);
        h.key("Enter");
    }

    /// ⌃k › Import Data Sources… › a found location › the confirm: ↓ moves over the skipped rows, Space leaves rows out,
    /// y imports only the checked ones (fresh ids, MCP Off, no password)
    #[tokio::test]
    async fn palette_location_toggle_and_import_only_the_checked() {
        let mut h = Harness::new().await;
        h.app.import_home = fixtures().join("home");
        h.key("Ctrl+K");
        pick(&mut h, "Import Data Sources… (DataGrip");
        let shown = h.screen(120, 30);
        let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette: {}", h.app.status) };
        assert_eq!(p.title, "Import Data Sources from", "{shown}");
        let labels: Vec<&str> = p.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"DataGrip 2025.2 · global  (10)"));
        assert_eq!(labels.last(), Some(&"File or folder…"));
        let s = h.screen(120, 30);
        assert!(s.contains("Import Data Sources from") && s.contains("DBeaver · workspace6 General  (7)") && s.contains("~/Library/DBeaverData/workspace6/General/.dbeaver"), "{s}");

        pick(&mut h, "DBeaver · workspace6 General");
        let Some(Popup::ImportPick(p)) = &h.app.popup else { panic!("no confirm: {}", h.app.status) };
        assert_eq!((p.count(), p.plan.rows.len()), (5, 7));
        let s = h.screen(120, 40);
        assert!(s.contains("Confirm") && s.contains("Import these data sources from DBeaver · workspace6 General?") && s.contains("[x] + add \"Sakila\""), "{s}");
        assert!(s.contains("− skip \"DB2 Sample\" (Db2)") && s.contains("Db2 isn't supported") && s.contains("y / Enter import 5"), "{s}");

        // Rows by name, added first: chinook, HR, logs, prod-replica, Sakila, then CSV exports and DB2 Sample (skipped).
        // Leave out chinook and prod-replica
        h.key(" ");
        h.key("Down");
        h.key("Down");
        h.key("Down");
        h.key(" ");
        for _ in 0..4 {
            h.key("Down");
        }
        let Some(Popup::ImportPick(p)) = &h.app.popup else { panic!() };
        assert!(p.plan.rows[p.sel].text.contains("Sakila"), "↓ stops at the last row with a box: {}", p.plan.rows[p.sel].text);
        let s = h.screen(120, 40);
        assert!(s.contains("[ ] + add \"prod-replica\"") && s.contains("[ ] + add \"chinook\"") && s.contains("y / Enter import 3"), "{s}");
        h.key("y");
        h.settle().await;
        assert!(h.app.popup.is_none(), "{}", h.app.status);
        assert_eq!(h.app.status, "Imported 3 data sources — passwords are asked on the first connect");
        let mut names: Vec<String> = h.app.state.store.list().unwrap().into_iter().map(|c| c.name).collect();
        names.sort();
        assert_eq!(names, ["HR", "Sakila", "logs"]);
        let c = h.app.state.store.list().unwrap().into_iter().find(|c| c.name == "Sakila").unwrap();
        assert!(!c.password_saved && c.options.mcp.as_deref() == Some("off") && !c.id.starts_with("mysql8-"));
        assert!(h.app.conns.iter().any(|c| c.config.name == "Sakila"), "the explorer lists them");
    }

    /// Nothing checked: y doesn't import; n / Esc cancels
    #[tokio::test]
    async fn nothing_checked_or_cancelled_imports_nothing() {
        let mut h = Harness::new().await;
        h.app.import_home = fixtures().join("home");
        h.app.ds_import_start();
        pick(&mut h, "project shop-api");
        for _ in 0..3 {
            h.key(" ");
            h.key("Down");
        }
        let Some(Popup::ImportPick(p)) = &h.app.popup else { panic!("{}", h.app.status) };
        assert_eq!(p.count(), 0);
        assert!(h.screen(120, 30).contains("y / Enter import 0"));
        h.key("Enter");
        assert!(matches!(&h.app.popup, Some(Popup::ImportPick(_))) && h.app.status.contains("Check at least one"), "{}", h.app.status);
        h.key("Esc");
        h.settle().await;
        assert!(h.app.popup.is_none() && h.app.state.store.list().unwrap().is_empty());
    }

    /// File or folder…: a typed path (also a Qurry export); a file that can't be read is a notice that says where
    #[tokio::test]
    async fn typed_path_and_errors() {
        let mut h = Harness::new().await;
        h.app.import_home = fixtures().join("home");
        h.app.ds_import_start();
        pick(&mut h, "File or folder");
        assert!(matches!(&h.app.popup, Some(Popup::Input { title, .. }) if title.starts_with("Import Data Sources from")));
        h.type_str("~/work/shop-api");
        h.key("Enter");
        let Some(Popup::ImportPick(p)) = &h.app.popup else { panic!("{}", h.app.status) };
        assert!(p.plan.message.starts_with("Import these data sources from DataGrip / IntelliJ · project shop-api?"), "{}", p.plan.message);
        h.key("n");

        h.app.ds_import_start();
        pick(&mut h, "File or folder");
        h.type_str(&fixtures().join("broken/dataSources.xml").display().to_string());
        h.key("Enter");
        let Some(Popup::Message { text, .. }) = &h.app.popup else { panic!("{}", h.app.status) };
        assert!(text.contains("line 7, column ") && text.contains("Nothing was imported."), "{text}");
    }

    /// Nobody's settings on this computer: the palette says so and keeps File or folder…
    #[tokio::test]
    async fn nothing_found() {
        let mut h = Harness::new().await;
        h.app.ds_import_start();
        let Some(Popup::Palette(p)) = &h.app.popup else { panic!() };
        assert_eq!(p.items.len(), 1);
        assert!(p.items[0].hint.starts_with("No DataGrip, IntelliJ or DBeaver settings found"));
    }
}
