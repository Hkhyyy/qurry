//! 데이터 내보내기 (형식 → 파일/클립보드 → 경로) 와 가져오기 (파일 → 컬럼 짝짓기 확인 → 넣기). 진행은 상태줄 막대
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind, byte_offset};
use crate::tree::NodeKind;
use qurry_core::sql;
use qurry_core::transfer::{Dest, Format, ImportSpec};
use tui_textarea::TextArea;

/// 내보낼 것: (접속 대상, SQL, 이름, SQL INSERT 대상 테이블)
type Source = (String, String, String, Option<(String, String)>);

/// 1234567 → "1,234,567"
pub fn thousands(n: u64) -> String {
    n.to_string().as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",")
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// "~/x" → 홈 아래
fn expand(p: &str) -> String {
    match p.trim().strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", home()),
        None => p.trim().to_string(),
    }
}

/// 파일 열 이름 → 같은 이름의 컬럼 (대소문자·공백·_·- 무시). GUI lib/transfer.ts autoMap 과 같다
fn match_column<'a>(file: &str, cols: &'a [String]) -> Option<&'a String> {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect::<String>();
    cols.iter().find(|c| norm(c) == norm(file))
}

impl App {
    /// 트리에서 고른 테이블, 아니면 지금 탭 (테이블: 걸러 둔 그대로, 콘솔: 커서의 문장)
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

    /// 1. 형식
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

    /// 2. 파일 / 클립보드
    pub fn export_dest(&mut self, f: Format) {
        let items = vec![
            PItem { label: "File…".into(), hint: "all rows, choose where to save".into(), action: PAction::ExportTo(f, false) },
            PItem { label: "Clipboard".into(), hint: "all rows (OSC 52 — large data may not fit your terminal)".into(), action: PAction::ExportTo(f, true) },
        ];
        self.chooser(&format!("Export {} — to", f.label()), items);
    }

    /// 3. 파일이면 경로를 묻고, 돌린다
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

    /// 상태줄에 올리고 진행이 바뀌는 동안 화면을 다시 그리게 한다
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

    /// 가져오기: 트리에서 고른 테이블·스키마(새 테이블), 아니면 지금 테이블 탭. 파일 경로를 묻는다
    pub fn import_start(&mut self) {
        // (대상, 스키마, 테이블, 이미 아는 컬럼)
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
            // 컬럼을 읽어 와서
            (Some(t), None) => {
                let st = self.state.clone();
                let (tg, sc, tb) = (target.clone(), schema.clone(), t.clone());
                self.spawn(async move { st.table_detail(tg, sc, tb).await }, move |a, r| match r {
                    Ok(d) => a.import_confirm(target, schema, Some(t), d.columns.into_iter().map(|c| c.name).collect(), pv, path),
                    Err(e) => a.status = format!("Import: {e}"),
                });
            }
            // 새 테이블: 파일 이름, 열 이름 그대로
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
                    // 열려 있는 그 테이블 탭은 다시 불러온다
                    let open: Vec<u64> = a.tabs.iter().filter(|t| matches!(&t.kind, TabKind::Table(tt) if tt.schema == schema && tt.table == table)).map(|t| t.id).collect();
                    for id in open {
                        a.reload_table(id);
                    }
                }
                Err(e) => a.status = format!("{label}: {e}"),
            }
        });
    }

    /// 백업·복원 대상: 트리에서 고른 Data Source·DB, 아니면 지금 탭
    fn backup_target(&self) -> Option<(String, String)> {
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected()
        {
            return Some((n.conn.clone(), n.db.clone().unwrap_or_else(|| self.conn(&n.conn).map(|c| sql::default_db(&c.config)).unwrap_or_default())));
        }
        self.tabs.get(self.active).map(|t| (t.conn.clone(), t.db.clone()))
    }

    /// ⌃K › Back Up Database…: 파일 경로 (.dump = PG custom, 나머지는 SQL) → pg_dump / mysqldump, 상태줄 진행
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
            let format = if path.ends_with(".dump") || path.ends_with(".backup") { "custom" } else { "plain" };
            let opts = qurry_core::backup::BackupOptions { format: format.into(), ..Default::default() };
            let label = format!("Back up {db}");
            let job = a.start_job(&label);
            let st = a.state.clone();
            let (target, j2, p2) = (sql::target_id(&conn, Some(&db)), job.clone(), path.clone());
            a.spawn(async move { st.backup(target, p2, opts, j2).await }, move |a, r| {
                a.end_job(&job);
                a.status = match r {
                    Ok(n) => format!("Backed up {:.1} MB to {path}", n as f64 / 1048576.0),
                    Err(e) => format!("{label}: {e}"),
                };
            });
        });
    }

    /// ⌃K › Restore Database…: 백업 파일 → 묻고 → pg_restore / psql / mysql
    pub fn restore_start(&mut self) {
        let Some((conn, db)) = self.backup_target() else {
            self.status = "Select a data source or database first".into();
            return;
        };
        let name = self.conn(&conn).map(|c| c.config.name.clone()).unwrap_or_default();
        let prod = self.conn(&conn).is_some_and(|c| c.config.options.confirm_changes);
        self.input("Restore from backup file (.dump or .sql)", &format!("{}/Downloads/", home()), move |a, p| {
            let path = expand(&p);
            a.popup = Some(Popup::Confirm {
                text: format!("Restore {path}\ninto {name} / {db}?\n\nObjects in the backup are dropped and recreated (pg_restore --clean); the rest of the database is left alone. One transaction for PostgreSQL.{}", if prod { "\n\n⚠ This is a Production data source." } else { "" }),
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

/// 오늘 날짜 (YYYY-MM-DD, 파일 이름용)
fn chrono_date() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    // 1970-01-01 부터 날 수 → 연·월·일 (Howard Hinnant 알고리즘)
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
