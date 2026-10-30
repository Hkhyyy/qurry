//! Data export (format → file/clipboard → path) and import (file → confirm column mapping → insert). Progress as a status-line bar
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind, byte_offset};
use crate::tree::NodeKind;
use qurry_core::sql;
use qurry_core::transfer::{Dest, Format, ImportSpec};
use tui_textarea::TextArea;

/// What to export: (connection target, SQL, name, target table for SQL INSERT)
type Source = (String, String, String, Option<(String, String)>);

/// 1234567 → "1,234,567"
pub fn thousands(n: u64) -> String {
    n.to_string().as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",")
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// "~/x" → under home
fn expand(p: &str) -> String {
    match p.trim().strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", home()),
        None => p.trim().to_string(),
    }
}

/// File column name → column with the same name (ignoring case·spaces·_·-). Same as autoMap in GUI lib/transfer.ts
fn match_column<'a>(file: &str, cols: &'a [String]) -> Option<&'a String> {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect::<String>();
    cols.iter().find(|c| norm(c) == norm(file))
}

impl App {
    /// The table picked in the tree, otherwise the current tab (table: as filtered, console: the statement at the cursor)
    fn export_source(&self) -> Option<Source> {
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected().filter(|n| n.is_relation())
        {
            let kind = self.kind_of(&n.conn);
            let (schema, name) = (n.schema.clone()?, n.name.clone()?);
            let q = format!("SELECT * FROM {}.{}", sql::quote_ident(kind, &schema), sql::quote_ident(kind, &name));
            return Some((sql::target_id(&n.conn, n.db.as_deref()), q, name.clone(), Some((schema, name))));
        }
        let t = self.tabs.get(self.active)?;
        let kind = self.kind_of(&t.conn);
        match &t.kind {
            TabKind::Table(tt) => {
                let w = if tt.where_.trim().is_empty() { String::new() } else { format!(" WHERE {}", tt.where_) };
                let q = format!("SELECT * FROM {}.{}{w}", sql::quote_ident(kind, &tt.schema), sql::quote_ident(kind, &tt.table));
                Some((t.target.clone(), q, tt.table.clone(), Some((tt.schema.clone(), tt.table.clone()))))
            }
            TabKind::Console(c) => {
                let text = c.editor.lines().join("\n");
                let (row, col) = c.editor.cursor();
                let q = sql::statement_at(&text, byte_offset(c.editor.lines(), row, col), kind)?;
                Some((t.target.clone(), q, "result".into(), None))
            }
            _ => None,
        }
    }

    fn chooser(&mut self, title: &str, items: Vec<PItem>) {
        self.popup = Some(Popup::Palette(Palette { title: title.into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// 1. Format
    pub fn export_start(&mut self) {
        if self.export_source().is_none() {
            self.status = "Select a table, or open a table or console tab to export".into();
            return;
        }
        let hint = |f: Format| match f {
            Format::Csv => "Excel, spreadsheets",
            Format::Tsv => "tab separated — pastes into spreadsheets as cells",
            Format::Json => "array of objects",
            Format::Sql => "one INSERT statement per row",
            Format::Markdown => "table for docs, issues, PRs",
            Format::Xlsx => "import only",
        };
        let items = Format::ALL.iter().map(|&f| PItem { label: f.label().into(), hint: hint(f).into(), action: PAction::ExportFormat(f) }).collect();
        self.chooser("Export Data — format", items);
    }

    /// 2. File / clipboard
    pub fn export_dest(&mut self, f: Format) {
        let items = vec![
            PItem { label: "File…".into(), hint: "all rows, choose where to save".into(), action: PAction::ExportTo(f, false) },
            PItem { label: "Clipboard".into(), hint: "all rows (OSC 52 — large data may not fit your terminal)".into(), action: PAction::ExportTo(f, true) },
        ];
        self.chooser(&format!("Export {} — to", f.label()), items);
    }

    /// 3. If a file, ask for the path, then run
    pub fn export_to(&mut self, f: Format, clipboard: bool) {
        let Some(src) = self.export_source() else { return };
        if clipboard {
            return self.export_run(src, f, Dest::Clipboard);
        }
        let name = src.2.split_whitespace().next().unwrap_or("result").to_string();
        self.input(format!("Export {} to file", f.label()), &format!("{}/Downloads/{name}.{}", home(), f.ext()), move |a, p| {
            a.export_run(src, f, Dest::File(expand(&p).into()))
        });
    }

    fn export_run(&mut self, (target, q, name, table): Source, f: Format, dest: Dest) {
        let label = format!("Export {name}");
        let to = match &dest {
            Dest::File(p) => format!(" to {}", p.display()),
            Dest::Clipboard => String::new(),
        };
        let job = self.start_job(&label);
        let st = self.state.clone();
        let j2 = job.clone();
        self.spawn(async move { st.export(target, q, f, dest, table, j2).await }, move |a, r| {
            a.end_job(&job);
            match r {
                Ok(e) => match e.text {
                    Some(text) => {
                        crate::input::copy(a, &text);
                        a.status = format!("Copied {} rows ({} KB) to the clipboard", thousands(e.rows), thousands(text.len() as u64 / 1024));
                    }
                    None => a.status = format!("Exported {} rows{to}", thousands(e.rows)),
                },
                Err(e) => a.status = format!("{label}: {e}"),
            }
        });
    }

    /// Put it on the status line and redraw the screen while the progress changes
    pub(crate) fn start_job(&mut self, label: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let job = self.state.start_job(&id);
        self.jobs.push((id.clone(), label.to_string(), job));
        let (st, tx, id2) = (self.state.clone(), self.tx.clone(), id.clone());
        tokio::spawn(async move {
            while st.job(&id2).is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                if tx.send(Box::new(|_: &mut App| {})).is_err() {
                    break;
                }
            }
        });
        id
    }

    pub(crate) fn end_job(&mut self, id: &str) {
        self.jobs.retain(|j| j.0 != id);
    }

    /// Import: the table·schema (new table) picked in the tree, otherwise the current table tab. Asks for the file path
    pub fn import_start(&mut self) {
        // (target, schema, table, already-known columns)
        type To = (String, String, Option<String>, Option<Vec<String>>);
        let mut to: Option<To> = None;
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected()
        {
            let target = sql::target_id(&n.conn, n.db.as_deref());
            to = match &n.kind {
                NodeKind::Object(k) if ["table", "partitioned"].contains(&k.as_str()) => Some((target, n.schema.clone().unwrap_or_default(), n.name.clone(), None)),
                NodeKind::Schema => Some((target, n.schema.clone().unwrap_or_default(), None, None)),
                _ => None,
            };
        }
        if to.is_none()
            && let Some(Tab { kind: TabKind::Table(tt), target, .. }) = self.tabs.get(self.active)
        {
            let cols = tt.grid.as_ref().map(|g| g.result.columns.iter().map(|c| c.name.clone()).collect());
            to = Some((target.clone(), tt.schema.clone(), Some(tt.table.clone()), cols));
        }
        let Some((target, schema, table, cols)) = to else {
            self.status = "Select a table (or a schema for a new table), or open a table tab to import into".into();
            return;
        };
        self.input("Import CSV / TSV / JSON / Excel file", &format!("{}/Downloads/", home()), move |a, p| a.import_file(target, schema, table, cols, expand(&p)));
    }

    fn import_file(&mut self, target: String, schema: String, table: Option<String>, cols: Option<Vec<String>>, path: String) {
        let pv = match qurry_core::transfer::preview(std::path::Path::new(&path), 3) {
            Ok(pv) => pv,
            Err(e) => return self.status = format!("Import: {e}"),
        };
        match (table, cols) {
            (Some(t), Some(c)) => self.import_confirm(target, schema, Some(t), c, pv, path),
            // Read the columns, then
            (Some(t), None) => {
                let st = self.state.clone();
                let (tg, sc, tb) = (target.clone(), schema.clone(), t.clone());
                self.spawn(async move { st.table_detail(tg, sc, tb).await }, move |a, r| match r {
                    Ok(d) => a.import_confirm(target, schema, Some(t), d.columns.into_iter().map(|c| c.name).collect(), pv, path),
                    Err(e) => a.status = format!("Import: {e}"),
                });
            }
            // New table: file name, column names as is
            (None, _) => {
                let cols = pv.columns.clone();
                self.import_confirm(target, schema, None, cols, pv, path)
            }
        }
    }

    fn import_confirm(&mut self, target: String, schema: String, table: Option<String>, cols: Vec<String>, pv: qurry_core::transfer::Preview, path: String) {
        let create = table.is_none();
        let file = std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let table = table.unwrap_or_else(|| {
            let stem = std::path::Path::new(&file).file_stem().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
            stem.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
        });
        let columns: Vec<(usize, String)> = pv.columns.iter().enumerate().filter_map(|(i, f)| match_column(f, &cols).map(|c| (i, c.clone()))).collect();
        let lines: Vec<String> = pv
            .columns
            .iter()
            .enumerate()
            .map(|(i, f)| format!("  {f} → {}", columns.iter().find(|c| c.0 == i).map(|c| c.1.as_str()).unwrap_or("(skip)")))
            .collect();
        if columns.is_empty() {
            self.popup = Some(Popup::Message { title: "Import".into(), text: format!("No column of {file} matches a column of {schema}.{table}:\n{}", lines.join("\n")) });
            return;
        }
        let text = format!(
            "Import {file} ({}, {:.1} MB) into {}{schema}.{table}?\n\n{}\n\nEmpty cells become NULL. One transaction: if a row fails, nothing is imported.",
            pv.format.label(),
            pv.bytes as f64 / 1048576.0,
            if create { "a new table " } else { "" },
            lines.join("\n")
        );
        let spec = ImportSpec { path, schema, table, create, columns };
        self.popup = Some(Popup::Confirm { text, on_yes: Box::new(move |a| a.import_run(target, spec, file)) });
    }

    fn import_run(&mut self, target: String, spec: ImportSpec, file: String) {
        let label = format!("Import {file}");
        let job = self.start_job(&label);
        let st = self.state.clone();
        let (j2, dest) = (job.clone(), format!("{}.{}", spec.schema, spec.table));
        let (create, schema, table) = (spec.create, spec.schema.clone(), spec.table.clone());
        self.spawn(async move { st.import(target, spec, j2).await }, move |a, r| {
            a.end_job(&job);
            match r {
                Ok(n) => {
                    a.status = format!("Imported {} rows into {dest}", thousands(n));
                    if create {
                        a.status += " (Refresh the explorer to see the new table)";
                    }
                    // Reload that table's tab if it's open
                    let open: Vec<u64> = a.tabs.iter().filter(|t| matches!(&t.kind, TabKind::Table(tt) if tt.schema == schema && tt.table == table)).map(|t| t.id).collect();
                    for id in open {
                        a.reload_table(id);
                    }
                }
                Err(e) => a.status = format!("{label}: {e}"),
            }
        });
    }

    /// Backup·restore target: the Data Source·DB picked in the tree, otherwise the current tab
    fn backup_target(&self) -> Option<(String, String)> {
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected()
        {
            return Some((n.conn.clone(), n.db.clone().unwrap_or_else(|| self.conn(&n.conn).map(|c| sql::default_db(&c.config)).unwrap_or_default())));
        }
        self.tabs.get(self.active).map(|t| (t.conn.clone(), t.db.clone()))
    }

    /// ⌃K › Back Up Database…: file path (.dump = PG custom, others SQL) → MySQL: leave out DEFINER= clauses? → pg_dump / mysqldump, progress on the status line
    pub fn backup_start(&mut self) {
        let Some((conn, db)) = self.backup_target() else {
            self.status = "Select a data source or database first".into();
            return;
        };
        let pg = self.kind_of(&conn) == qurry_core::store::DbKind::Postgres;
        let date = chrono_date();
        let def = format!("{}/Downloads/{db}-{date}.{}", home(), if pg { "dump" } else { "sql" });
        self.input("Back up to (.dump = PostgreSQL custom format, .sql = SQL)", &def, move |a, p| {
            let path = expand(&p);
            if pg {
                return a.backup_run(conn, db, path, false);
            }
            a.input("Leave out DEFINER= clauses? y / n (y to restore on RDS and other managed services, where another user's DEFINER needs SUPER)", "n", move |a, yn| {
                a.backup_run(conn, db, path, yn.trim().eq_ignore_ascii_case("y"))
            });
        });
    }

    fn backup_run(&mut self, conn: String, db: String, path: String, strip_definers: bool) {
        let format = if path.ends_with(".dump") || path.ends_with(".backup") { "custom" } else { "plain" };
        let opts = qurry_core::backup::BackupOptions { format: format.into(), strip_definers, ..Default::default() };
        let label = format!("Back up {db}");
        let job = self.start_job(&label);
        let st = self.state.clone();
        let (target, j2, p2) = (sql::target_id(&conn, Some(&db)), job.clone(), path.clone());
        self.spawn(async move { st.backup(target, p2, opts, j2).await }, move |a, r| {
            a.end_job(&job);
            a.status = match r {
                Ok(n) => format!("Backed up {:.1} MB to {path}", n as f64 / 1048576.0),
                Err(e) => format!("{label}: {e}"),
            };
        });
    }

    /// ⌃K › Restore Database…: backup file → confirm → pg_restore / psql / mysql
    pub fn restore_start(&mut self) {
        let Some((conn, db)) = self.backup_target() else {
            self.status = "Select a data source or database first".into();
            return;
        };
        let name = self.conn(&conn).map(|c| c.config.name.clone()).unwrap_or_default();
        let prod = self.conn(&conn).is_some_and(|c| c.config.options.confirm_changes);
        let how = if self.kind_of(&conn) == qurry_core::store::DbKind::Mysql {
            "Objects in the backup are dropped and recreated (by the backup's DROP … IF EXISTS statements); the rest of the database is left alone. A MySQL restore isn't atomic: if a statement fails, what ran before it stays."
        } else {
            "Objects in the backup are dropped and recreated (pg_restore --clean); the rest of the database is left alone. One transaction: if a statement fails, nothing changes."
        };
        self.input("Restore from backup file (.dump or .sql)", &format!("{}/Downloads/", home()), move |a, p| {
            let path = expand(&p);
            a.popup = Some(Popup::Confirm {
                text: format!("Restore {path}\ninto {name} / {db}?\n\n{how}{}", if prod { "\n\n⚠ This is a Production data source." } else { "" }),
                on_yes: Box::new(move |a| {
                    let label = format!("Restore into {db}");
                    let job = a.start_job(&label);
                    let st = a.state.clone();
                    let (target, j2, p2) = (sql::target_id(&conn, Some(&db)), job.clone(), path.clone());
                    a.spawn(async move { st.restore(target, p2, qurry_core::backup::RestoreOptions { clean: true }, j2).await }, move |a, r| {
                        a.end_job(&job);
                        a.status = match r {
                            Ok(()) => format!("Restored {path} into {db}"),
                            Err(e) => format!("{label}: {e}"),
                        };
                    });
                }),
            });
        });
    }
}

/// Today's date (YYYY-MM-DD, for file names)
fn chrono_date() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    // Days since 1970-01-01 → year·month·day (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use crate::app::{Focus, Popup};
    use crate::testkit::Harness;

    /// A harness with one data source (from JSON) and a console on this DB, so ⌃K › Back Up Database… targets it
    async fn harness(cfg: serde_json::Value, password: Option<&str>, db: &str) -> Harness {
        let mut h = Harness::new().await;
        let id = cfg["id"].as_str().unwrap().to_string();
        h.app.state.save_connection(serde_json::from_value(cfg).unwrap(), password.map(String::from), None).await.unwrap();
        h.app.reload_conns();
        h.settle().await;
        let tab = h.app.restore_console(&id, db.into(), "console".into(), "").unwrap();
        let i = h.app.tab_index(tab).unwrap();
        h.app.activate(i);
        h.app.focus = Focus::Editor;
        h
    }

    /// Answer the open input popup with this text
    fn answer(h: &mut Harness, text: &str) {
        let Some(Popup::Input { input, .. }) = h.app.popup.as_mut() else { panic!("no input popup: {}", h.app.status) };
        *input = tui_textarea::TextArea::new(vec![text.to_string()]);
        h.key("Enter");
    }

    fn popup_title(h: &Harness) -> Option<String> {
        match &h.app.popup {
            Some(Popup::Input { title, .. }) => Some(title.clone()),
            _ => None,
        }
    }

    /// MySQL backups ask about DEFINER= clauses after the path; PostgreSQL backups start right away
    #[tokio::test]
    async fn mysql_backup_asks_about_definers() {
        let my = serde_json::json!({ "id": "m", "name": "m", "kind": "mysql", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop" });
        let mut h = harness(my, None, "shop").await;
        h.app.backup_start();
        assert!(popup_title(&h).unwrap().starts_with("Back up to"));
        answer(&mut h, "/surely/not/a/dir/shop.sql");
        assert!(popup_title(&h).unwrap().starts_with("Leave out DEFINER= clauses?"), "{:?}", popup_title(&h));
        // The restore confirmation names no PostgreSQL tool for MySQL
        h.app.popup = None;
        h.app.restore_start();
        answer(&mut h, "/surely/not/a/dir/shop.sql");
        let Some(Popup::Confirm { text, .. }) = &h.app.popup else { panic!("no confirm") };
        assert!(text.contains("DROP … IF EXISTS") && text.contains("isn't atomic") && !text.contains("pg_restore"), "{text}");
        let pg = serde_json::json!({ "id": "p", "name": "p", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "postgres" });
        let mut h = harness(pg, None, "postgres").await;
        h.app.backup_start();
        answer(&mut h, "/surely/not/a/dir/p.dump");
        assert!(h.app.popup.is_none());
    }

    /// The answer reaches the backup: y leaves the DEFINER= clauses out of the file, n keeps them (QURRY_TEST_MY, mysqldump in PATH)
    #[tokio::test]
    #[ignore]
    async fn mysql_backup_leaves_out_definers_when_asked() {
        let Ok(e) = std::env::var("QURRY_TEST_MY") else { return };
        if qurry_core::backup::find_tool("mysqldump").is_none() {
            return;
        }
        let p: Vec<&str> = e.split(':').collect();
        let cfg = serde_json::json!({ "id": "m", "name": "m", "kind": "mysql", "host": p[0], "port": p[1].parse::<u16>().unwrap(), "user": p[2], "database": p[4] });
        let mut h = harness(cfg, Some(p[3]), "qurry_mysqldump_tui").await;
        let st = h.app.state.clone();
        st.run_query("m".into(), "create database if not exists qurry_mysqldump_tui".into(), None, None).await.unwrap();
        for s in ["create table if not exists t (id int primary key)", "create or replace view v as select id from t"] {
            st.run_query("m::qurry_mysqldump_tui".into(), s.into(), None, None).await.unwrap();
        }
        let dir = std::env::temp_dir().join(format!("qurry-tui-bak-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for (yn, definers) in [("y", 0), ("n", 1)] {
            let file = dir.join(format!("{yn}.sql"));
            h.app.backup_start();
            answer(&mut h, &file.to_string_lossy());
            answer(&mut h, yn);
            for _ in 0..600 {
                h.settle().await;
                if h.app.jobs.is_empty() {
                    break;
                }
            }
            assert!(h.app.status.starts_with("Backed up"), "{}", h.app.status);
            assert_eq!(std::fs::read_to_string(&file).unwrap().matches("DEFINER=`").count(), definers, "{yn}");
        }
        st.run_query("m".into(), "drop database qurry_mysqldump_tui".into(), None, None).await.ok();
        std::fs::remove_dir_all(dir).ok();
    }
}
