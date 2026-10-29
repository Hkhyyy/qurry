//! 객체 만들기 · 고치기 (⌃k › New Table… · Modify Table… · New View / Schema / Database / Sequence… · Rename…).
//! 테이블은 디자이너 창: 컬럼 · 인덱스 · FK 목록을 키로 고치고, 아래에 만들어질 SQL 이 바로 보인다.
//! 한 줄은 SQL 과 비슷하게 쓴다 — 컬럼 `email varchar(100) not null default 'x' auto pk comment 'login'`,
//! 인덱스 `users_email_idx (email) unique`, FK `orders_user_fk (user_id) -> public.users (id) on delete cascade`
use crate::app::{App, Popup};
use crate::kit::*;
use crate::tree::NodeKind;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use qurry_core::ddl::{ColumnDef, ForeignKeyDef, IndexDef, NewObject, TableDef};
use qurry_core::store::DbKind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub struct Designer {
    pub target: String,
    pub conn: String,
    pub kind: DbKind,
    pub before: Option<TableDef>,
    pub def: TableDef,
    /// 컬럼 → 인덱스 → FK 를 이어서 센 줄
    pub sel: usize,
    pub sql: Result<Vec<String>, String>,
    pub message: Option<String>,
}

// ───────── 한 줄 글 ↔ 정의 ─────────

/// 공백으로 나누되 '…' · (…) 안은 한 덩어리
fn tokens(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quote) = (vec![], String::new(), 0i32, false);
    for c in s.chars() {
        match c {
            '\'' => quote = !quote,
            '(' if !quote => depth += 1,
            ')' if !quote => depth -= 1,
            c if c.is_whitespace() && !quote && depth <= 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> String {
    s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')).map(|x| x.replace("''", "'")).unwrap_or_else(|| s.to_string())
}

fn names(s: &str) -> Vec<String> {
    s.trim().trim_start_matches('(').trim_end_matches(')').split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

const COL_WORDS: &[&str] = &["not", "null", "default", "auto", "pk", "comment"];

/// `name type [not null | null] [default expr] [auto] [pk] [comment 'text']` → (컬럼, PK 인지)
pub fn parse_column(spec: &str) -> Result<(ColumnDef, bool), String> {
    let t = tokens(spec);
    if t.len() < 2 {
        return Err("Column: write name type [not null] [default …] [auto] [pk] [comment '…']".into());
    }
    let is_kw = |s: &str| COL_WORDS.contains(&s.to_lowercase().as_str());
    let mut i = 1;
    let mut ty = vec![];
    while i < t.len() && !is_kw(&t[i]) {
        ty.push(t[i].clone());
        i += 1;
    }
    if ty.is_empty() {
        return Err("Give the column a type".into());
    }
    let mut c = ColumnDef { name: t[0].clone(), data_type: ty.join(" "), nullable: true, ..Default::default() };
    let mut pk = false;
    while i < t.len() {
        match t[i].to_lowercase().as_str() {
            "not" if t.get(i + 1).is_some_and(|x| x.eq_ignore_ascii_case("null")) => {
                c.nullable = false;
                i += 2;
            }
            "null" => {
                c.nullable = true;
                i += 1;
            }
            "auto" => {
                c.auto_increment = true;
                i += 1;
            }
            "pk" => {
                pk = true;
                c.nullable = false;
                i += 1;
            }
            "default" => {
                let mut v = vec![];
                i += 1;
                while i < t.len() && !is_kw(&t[i]) {
                    v.push(t[i].clone());
                    i += 1;
                }
                c.default = Some(v.join(" ")).filter(|x| !x.is_empty());
            }
            "comment" => {
                c.comment = t.get(i + 1).map(|x| unquote(x)).filter(|x| !x.is_empty());
                i += 2;
            }
            w => return Err(format!("Don't understand \"{w}\"")),
        }
    }
    Ok((c, pk))
}

pub fn column_spec(c: &ColumnDef, pk: bool) -> String {
    let mut s = format!("{} {}", c.name, c.data_type);
    if !c.nullable && !pk {
        s.push_str(" not null");
    }
    if let Some(d) = c.default.as_deref().filter(|d| !d.is_empty()) {
        s.push_str(&format!(" default {d}"));
    }
    if c.auto_increment {
        s.push_str(" auto");
    }
    if pk {
        s.push_str(" pk");
    }
    if let Some(cm) = c.comment.as_deref().filter(|d| !d.is_empty()) {
        s.push_str(&format!(" comment '{}'", cm.replace('\'', "''")));
    }
    s
}

/// `name (a, b) [unique]`
pub fn parse_index(spec: &str) -> Result<IndexDef, String> {
    let t = tokens(spec);
    match t.as_slice() {
        [name, cols, rest @ ..] if cols.starts_with('(') && rest.iter().all(|r| r.eq_ignore_ascii_case("unique")) => Ok(IndexDef { name: name.clone(), columns: names(cols), unique: !rest.is_empty(), ..Default::default() }),
        _ => Err("Index: write name (column, …) [unique]".into()),
    }
}

pub fn index_spec(i: &IndexDef) -> String {
    format!("{} ({}){}", i.name, i.columns.join(", "), if i.unique { " unique" } else { "" })
}

/// `name (cols) -> [schema.]table (cols) [on delete X] [on update Y]`
pub fn parse_fk(spec: &str, schema: &str) -> Result<ForeignKeyDef, String> {
    let bad = || "Foreign key: write name (column) -> table (column) [on delete cascade] [on update …]".to_string();
    let (left, right) = spec.split_once("->").ok_or_else(bad)?;
    let l = tokens(left);
    let [name, cols] = l.as_slice() else { return Err(bad()) };
    let r = tokens(right);
    if r.len() < 2 || !r[1].starts_with('(') {
        return Err(bad());
    }
    let (rs, rt) = match r[0].split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (schema.to_string(), r[0].clone()),
    };
    let mut f = ForeignKeyDef { name: name.clone(), columns: names(cols), ref_schema: rs, ref_table: rt, ref_columns: names(&r[1]), on_delete: "NO ACTION".into(), on_update: "NO ACTION".into(), ..Default::default() };
    let rest: Vec<String> = r[2..].iter().map(|x| x.to_uppercase()).collect();
    let mut i = 0;
    while i < rest.len() {
        if rest[i] != "ON" || i + 2 >= rest.len() + 1 {
            return Err(bad());
        }
        let which = rest.get(i + 1).ok_or_else(bad)?.clone();
        // SET NULL · SET DEFAULT · NO ACTION 은 두 낱말
        let two = matches!(rest.get(i + 2).map(String::as_str), Some("SET" | "NO"));
        let action = if two { format!("{} {}", rest.get(i + 2).unwrap(), rest.get(i + 3).ok_or_else(bad)?) } else { rest.get(i + 2).ok_or_else(bad)?.clone() };
        match which.as_str() {
            "DELETE" => f.on_delete = action,
            "UPDATE" => f.on_update = action,
            _ => return Err(bad()),
        }
        i += if two { 4 } else { 3 };
    }
    Ok(f)
}

pub fn fk_spec(f: &ForeignKeyDef, schema: &str) -> String {
    let rt = if f.ref_schema.is_empty() || f.ref_schema == schema { f.ref_table.clone() } else { format!("{}.{}", f.ref_schema, f.ref_table) };
    let mut s = format!("{} ({}) -> {rt} ({})", f.name, f.columns.join(", "), f.ref_columns.join(", "));
    for (w, v) in [("delete", &f.on_delete), ("update", &f.on_update)] {
        if !v.is_empty() && v != "NO ACTION" {
            s.push_str(&format!(" on {w} {}", v.to_lowercase()));
        }
    }
    s
}

// ───────── 디자이너 ─────────

enum Row {
    Col(usize),
    Idx(usize),
    Fk(usize),
}

impl Designer {
    fn rows(&self) -> Vec<Row> {
        let d = &self.def;
        (0..d.columns.len()).map(Row::Col).chain((0..d.indexes.len()).map(Row::Idx)).chain((0..d.foreign_keys.len()).map(Row::Fk)).collect()
    }

    pub fn refresh(&mut self) {
        let r = match &self.before {
            Some(b) => qurry_core::ddl::alter_table_sql(self.kind, b, &self.def),
            None => qurry_core::ddl::create_table_sql(self.kind, &self.def),
        };
        self.sql = r.map_err(|e| e.to_string());
        let n = self.rows().len();
        if self.sel >= n {
            self.sel = n.saturating_sub(1);
        }
    }

    fn pk(&self, name: &str) -> bool {
        self.def.primary_key.iter().any(|x| x == name)
    }

    /// 컬럼 이름이 바뀌면 PK · 인덱스 · FK 도 따라간다
    fn rename_col(&mut self, old: &str, new: &str) {
        let ren = |xs: &mut Vec<String>| xs.iter_mut().filter(|x| *x == old).for_each(|x| *x = new.to_string());
        ren(&mut self.def.primary_key);
        self.def.indexes.iter_mut().for_each(|i| ren(&mut i.columns));
        self.def.foreign_keys.iter_mut().for_each(|f| ren(&mut f.columns));
    }
}

/// 입력을 받고 디자이너로 돌아온다 (입력 팝업이 디자이너 위에 뜬다)
fn ask(app: &mut App, title: &str, init: &str, apply: impl FnOnce(&mut Designer, String) -> Result<(), String> + 'static) {
    app.input(title, init, move |a, v| {
        if let Some(d) = a.designer.as_mut() {
            d.message = apply(d, v).err();
            d.refresh();
        }
    });
}

pub fn key(app: &mut App, k: KeyEvent) {
    let Some(d) = app.designer.as_mut() else { return };
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let rows = d.rows();
    let schema = d.def.schema.clone();
    match k.code {
        KeyCode::Esc => app.designer = None,
        KeyCode::Down | KeyCode::Char('j') if !ctrl => d.sel = (d.sel + 1).min(rows.len().saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') if !ctrl => d.sel = d.sel.saturating_sub(1),
        KeyCode::Char('a') if !ctrl => ask(app, "Add column — name type [not null] [default …] [auto] [pk] [comment '…']", "", |d, v| {
            let (c, pk) = parse_column(&v)?;
            if pk {
                d.def.primary_key.push(c.name.clone());
            }
            d.def.columns.push(c);
            d.sel = d.def.columns.len() - 1;
            Ok(())
        }),
        KeyCode::Char('i') if !ctrl => {
            let init = format!("{}_idx{} ", d.def.name, d.def.indexes.len() + 1);
            ask(app, "Add index — name (column, …) [unique]", &init, |d, v| {
                d.def.indexes.push(parse_index(&v)?);
                d.sel = d.def.columns.len() + d.def.indexes.len() - 1;
                Ok(())
            })
        }
        KeyCode::Char('f') if !ctrl => {
            let init = format!("{}_fk{} ", d.def.name, d.def.foreign_keys.len() + 1);
            ask(app, "Add foreign key — name (column) -> table (column) [on delete cascade]", &init, move |d, v| {
                d.def.foreign_keys.push(parse_fk(&v, &schema)?);
                d.sel = d.rows().len() - 1;
                Ok(())
            })
        }
        KeyCode::Enter | KeyCode::Char('e') if !ctrl => match rows.get(d.sel) {
            Some(Row::Col(i)) => {
                let i = *i;
                let c = d.def.columns[i].clone();
                let spec = column_spec(&c, d.pk(&c.name));
                ask(app, "Edit column — name type [not null] [default …] [auto] [pk] [comment '…']", &spec, move |d, v| {
                    let (n, pk) = parse_column(&v)?;
                    let old = d.def.columns[i].name.clone();
                    if n.name != old {
                        d.rename_col(&old, &n.name);
                    }
                    let was = d.pk(&n.name);
                    if pk && !was {
                        d.def.primary_key.push(n.name.clone());
                    } else if !pk && was {
                        d.def.primary_key.retain(|x| *x != n.name);
                    }
                    // 지금 DB 의 이름 (orig) 은 그대로 → 이름 바꾸기 · 고치기가 된다
                    d.def.columns[i] = ColumnDef { orig: d.def.columns[i].orig.clone(), ..n };
                    Ok(())
                });
            }
            Some(Row::Idx(i)) => {
                let i = *i;
                if d.def.indexes[i].definition.is_some() {
                    d.message = Some("This index uses an expression or a condition — it can only be removed (d)".into());
                    return;
                }
                let spec = index_spec(&d.def.indexes[i]);
                ask(app, "Edit index — name (column, …) [unique]", &spec, move |d, v| {
                    let n = parse_index(&v)?;
                    d.def.indexes[i] = IndexDef { orig: d.def.indexes[i].orig.clone(), constraint: d.def.indexes[i].constraint, ..n };
                    Ok(())
                });
            }
            Some(Row::Fk(i)) => {
                let i = *i;
                let spec = fk_spec(&d.def.foreign_keys[i], &schema);
                ask(app, "Edit foreign key — name (column) -> table (column) [on delete …]", &spec, move |d, v| {
                    let n = parse_fk(&v, &schema)?;
                    d.def.foreign_keys[i] = ForeignKeyDef { orig: d.def.foreign_keys[i].orig.clone(), ..n };
                    Ok(())
                });
            }
            None => {}
        },
        KeyCode::Char('d') | KeyCode::Delete if !ctrl => {
            match rows.get(d.sel) {
                Some(Row::Col(i)) => {
                    let c = d.def.columns.remove(*i);
                    d.def.primary_key.retain(|x| *x != c.name);
                }
                Some(Row::Idx(i)) => {
                    d.def.indexes.remove(*i);
                }
                Some(Row::Fk(i)) => {
                    d.def.foreign_keys.remove(*i);
                }
                None => {}
            }
            d.refresh();
        }
        KeyCode::Char('p') if !ctrl => {
            if let Some(Row::Col(i)) = rows.get(d.sel) {
                let n = d.def.columns[*i].name.clone();
                if d.pk(&n) {
                    d.def.primary_key.retain(|x| *x != n);
                } else {
                    d.def.primary_key.push(n);
                    d.def.columns[*i].nullable = false;
                }
                d.refresh();
            }
        }
        KeyCode::Char('n') if !ctrl => {
            let name = d.def.name.clone();
            ask(app, "Table name", &name, |d, v| {
                d.def.name = v.trim().to_string();
                Ok(())
            });
        }
        KeyCode::Char('c') if !ctrl => {
            let cm = d.def.comment.clone().unwrap_or_default();
            ask(app, "Table comment", &cm, |d, v| {
                d.def.comment = Some(v).filter(|x| !x.trim().is_empty());
                Ok(())
            });
        }
        KeyCode::Char('o') if !ctrl => {
            if let Ok(sql) = d.sql.clone()
                && !sql.is_empty()
            {
                let (conn, db) = (d.conn.clone(), qurry_core::app::split_target(&d.target).1.map(String::from));
                app.designer = None;
                app.open_console(&conn, db);
                if let Some(crate::app::Tab { kind: crate::app::TabKind::Console(c), .. }) = app.tabs.get_mut(app.active) {
                    c.editor.insert_str(sql.iter().map(|s| format!("{s};")).collect::<Vec<_>>().join("\n\n"));
                }
            }
        }
        KeyCode::F(5) => apply(app),
        KeyCode::Char('s') if ctrl => apply(app),
        _ => {}
    }
}

/// 실행: 운영이면 묻고, PG 는 한 트랜잭션
fn apply(app: &mut App) {
    let Some(d) = app.designer.as_ref() else { return };
    let sql = match &d.sql {
        Ok(s) if !s.is_empty() => s.clone(),
        Ok(_) => return,
        Err(e) => {
            let e = e.clone();
            app.designer.as_mut().unwrap().message = Some(e);
            return;
        }
    };
    let (target, conn, name, created) = (d.target.clone(), d.conn.clone(), d.def.name.clone(), d.before.is_none());
    let db = qurry_core::app::split_target(&target).1.map(String::from).unwrap_or_else(|| app.default_db(&conn));
    let run = move |a: &mut App| {
        let st = a.state.clone();
        let n = sql.len();
        a.status = "Running…".into();
        a.spawn(async move { st.run_ddl(target, sql).await }, move |a, r| match r {
            Ok(()) => {
                a.designer = None;
                a.status = if created { format!("Table {name} created") } else { format!("Table {name} modified ({n} statement(s))") };
                a.schema_changed(&conn, &db);
            }
            Err(e) => {
                a.status.clear();
                if let Some(d) = a.designer.as_mut() {
                    d.message = Some(e.to_string());
                }
            }
        });
    };
    match app.production_name(&d.conn) {
        Some(p) => {
            let text = format!("Production data source \"{p}\" — this changes the schema:\n\n{}\n\nRun it?", d.sql.as_ref().unwrap().iter().map(|s| format!("  {}", s.split_whitespace().collect::<Vec<_>>().join(" "))).collect::<Vec<_>>().join("\n"));
            app.popup = Some(Popup::Confirm { text, on_yes: Box::new(run) });
        }
        None => run(app),
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(d) = app.designer.as_ref() else { return };
    let w = 110u16.min(area.width.saturating_sub(2));
    let r = centered(area, w, area.height.saturating_sub(2));
    let title = if d.before.is_some() { format!(" Modify Table: {} ", d.before.as_ref().unwrap().name) } else { " New Table ".into() };
    let inner = popup(f, r, title);
    let mut lines: Vec<Line> = vec![];
    let schema = if d.def.schema.is_empty() { String::new() } else { format!("{}.", d.def.schema) };
    lines.push(Line::from(vec![Span::styled(" Name  ", Style::default().fg(DIM)), Span::styled(format!("{schema}{}", if d.def.name.is_empty() { "(n to name it)" } else { &d.def.name }), Style::default().add_modifier(Modifier::BOLD)), Span::styled(d.def.comment.as_deref().map(|c| format!("   — {c}")).unwrap_or_default(), Style::default().fg(DIM))]));
    lines.push(Line::raw(""));
    let rows = d.rows();
    let name_w = d.def.columns.iter().map(|c| c.name.chars().count()).max().unwrap_or(4).clamp(4, 28);
    let line = |i: usize, text: String| {
        let sel = i == d.sel;
        Line::styled(format!("{}{text}", if sel { "▸ " } else { "  " }), if sel { Style::default().bg(selected(true)) } else { Style::default() })
    };
    lines.push(section(format!("Columns ({})", d.def.columns.len())));
    for (i, row) in rows.iter().enumerate() {
        match row {
            Row::Col(c) => {
                let c = &d.def.columns[*c];
                let mut flags = vec![];
                if d.pk(&c.name) {
                    flags.push("PK".to_string());
                }
                if !c.nullable {
                    flags.push("NOT NULL".into());
                }
                if c.auto_increment {
                    flags.push("auto".into());
                }
                if let Some(df) = c.default.as_deref().filter(|x| !x.is_empty()) {
                    flags.push(format!("default {df}"));
                }
                if c.orig.is_none() && d.before.is_some() {
                    flags.push("new".into());
                }
                lines.push(line(i, format!("{:name_w$}  {:22} {}", c.name, c.data_type, flags.join(" · "))));
            }
            Row::Idx(x) => {
                if *x == 0 {
                    lines.push(Line::raw(""));
                    lines.push(section(format!("Indexes ({})", d.def.indexes.len())));
                }
                let ix = &d.def.indexes[*x];
                lines.push(line(i, ix.definition.clone().unwrap_or_else(|| index_spec(ix))));
            }
            Row::Fk(x) => {
                if *x == 0 {
                    lines.push(Line::raw(""));
                    lines.push(section(format!("Foreign keys ({})", d.def.foreign_keys.len())));
                }
                lines.push(line(i, fk_spec(&d.def.foreign_keys[*x], &d.def.schema)));
            }
        }
    }
    lines.push(Line::raw(""));
    lines.push(section("SQL"));
    match &d.sql {
        Ok(s) if s.is_empty() => lines.push(Line::styled("  -- no changes", Style::default().fg(DIM))),
        Ok(s) => {
            for st in s {
                for l in format!("{st};").lines() {
                    lines.push(Line::styled(format!("  {l}"), Style::default().fg(ACCENT)));
                }
            }
        }
        Err(e) => lines.push(Line::styled(format!("  {e}"), Style::default().fg(DIM))),
    }
    if let Some(m) = &d.message {
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!("  {m}"), Style::default().fg(DANGER)));
    }
    let hint = key_hints(&[("a", "column"), ("i", "index"), ("f", "FK"), ("↩", "edit"), ("d", "remove"), ("p", "PK"), ("n", "name"), ("c", "comment"), ("F5", if d.before.is_some() { "apply" } else { "create" }), ("o", "console"), ("Esc", "close")]);
    let h = inner.height.saturating_sub(1);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), Rect { height: h, ..inner });
    f.render_widget(Paragraph::new(hint), Rect { y: inner.y + h, height: 1, ..inner });
}

// ───────── 열기 ─────────

impl App {
    /// 고른 노드: (대상, Data Source, DB, 스키마) — MySQL 은 DB 가 곧 스키마, PG 는 public
    fn ddl_place(&self) -> Option<(String, String, Option<String>, String)> {
        let (conn, db, schema) = match (self.focus, self.tree.selected()) {
            (crate::app::Focus::Tree, Some(n)) => (n.conn.clone(), n.db.clone(), n.schema.clone()),
            _ => {
                let t = self.tabs.get(self.active)?;
                (t.conn.clone(), Some(t.db.clone()), match &t.kind {
                    crate::app::TabKind::Table(tt) => Some(tt.schema.clone()),
                    _ => None,
                })
            }
        };
        let kind = self.kind_of(&conn);
        if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
            return None;
        }
        let db2 = db.clone().unwrap_or_else(|| self.default_db(&conn));
        let schema = schema.unwrap_or_else(|| if kind == DbKind::Mysql { db2.clone() } else { "public".into() });
        Some((qurry_core::sql::target_id(&conn, db.as_deref()), conn, db, schema))
    }

    pub fn new_table(&mut self) {
        let Some((target, conn, _, schema)) = self.ddl_place() else {
            return self.status = "Select a PostgreSQL / MySQL data source, database or schema first".into();
        };
        let kind = self.kind_of(&conn);
        let id = ColumnDef { name: "id".into(), data_type: "bigint".into(), auto_increment: true, ..Default::default() };
        let mut d = Designer { target, conn, kind, before: None, def: TableDef { schema, columns: vec![id], primary_key: vec!["id".into()], ..Default::default() }, sel: 0, sql: Ok(vec![]), message: None };
        d.refresh();
        self.designer = Some(d);
        self.input("New table — name", "", |a, v| {
            if let Some(d) = a.designer.as_mut() {
                d.def.name = v.trim().to_string();
                d.refresh();
            }
        });
    }

    pub fn modify_table(&mut self) {
        let Some((target, conn, schema, table)) = self.chosen_table() else {
            return self.status = "Select a table in the explorer (or open a table tab) to modify".into();
        };
        let kind = self.kind_of(&conn);
        if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
            return self.status = "Modifying tables works for PostgreSQL and MySQL".into();
        }
        let st = self.state.clone();
        let t2 = target.clone();
        self.status = "Reading the table…".into();
        self.spawn(async move { st.table_def(t2, schema, table).await }, move |a, r| match r {
            Ok(def) => {
                a.status.clear();
                let mut d = Designer { target, conn, kind, before: Some(def.clone()), def, sel: 0, sql: Ok(vec![]), message: None };
                d.refresh();
                a.designer = Some(d);
            }
            Err(e) => a.status = format!("Modify table: {e}"),
        });
    }

    /// New ▸ Database · Schema · View · Sequence: 이름 (뷰는 쿼리도) → SQL 확인 → 실행
    pub fn new_object(&mut self, what: &'static str) {
        let Some((target, conn, db, schema)) = self.ddl_place() else {
            return self.status = "Select a PostgreSQL / MySQL data source, database or schema first".into();
        };
        let title = match what {
            "database" => "New database — name".to_string(),
            "schema" => "New schema — name".to_string(),
            "sequence" => format!("New sequence in {schema} — name"),
            _ => format!("New view in {schema} — name"),
        };
        self.input(title, "", move |a, name| {
            let label = name.clone();
            let go = move |a: &mut App, o: NewObject| {
                let target = if what == "database" { conn.clone() } else { target };
                let sql = match a.state.new_object_ddl(target.clone(), o) {
                    Ok(s) => s,
                    Err(e) => return a.status = e.to_string(),
                };
                a.confirm_ddl(target, conn, db, sql, format!("{what} {label} created"));
            };
            match what {
                "database" => go(a, NewObject::Database { name: name.clone(), charset: String::new() }),
                "schema" => go(a, NewObject::Schema { name: name.clone() }),
                "sequence" => go(a, NewObject::Sequence { schema, name: name.clone(), start: None, increment: None }),
                _ => {
                    let n2 = name.clone();
                    a.input(format!("New view {name} — SELECT …"), "SELECT ", move |a, query| go(a, NewObject::View { schema, name: n2, query, materialized: false }));
                }
            }
        });
    }

    /// Rename… (⇧F6): 테이블 · 뷰 · 시퀀스 · 스키마 · DB (PG)
    pub fn rename_object(&mut self) {
        let Some(n) = self.tree.selected().filter(|_| self.focus == crate::app::Focus::Tree) else {
            return self.status = "Select a table, view, sequence, schema or database in the explorer".into();
        };
        let (what, old) = match &n.kind {
            NodeKind::Object(k) => (k.clone(), n.name.clone()),
            NodeKind::Schema => ("schema".into(), n.schema.clone()),
            NodeKind::Db => ("db".into(), n.db.clone()),
            _ => (String::new(), None),
        };
        let Some(old) = old.filter(|_| !what.is_empty()) else {
            return self.status = "Select a table, view, sequence, schema or database in the explorer".into();
        };
        let target = if what == "db" { n.conn.clone() } else { qurry_core::sql::target_id(&n.conn, n.db.as_deref()) };
        let schema = n.schema.clone().or(n.db.clone()).unwrap_or_default();
        let (conn, db) = (n.conn.clone(), n.db.clone());
        self.input(format!("Rename {old} to"), &old.clone(), move |a, new| {
            if new.trim() == old {
                return;
            }
            match a.state.rename_ddl(target.clone(), what, schema, old.clone(), new.trim().into()) {
                Ok(sql) => a.confirm_ddl(target, conn, db, vec![sql], format!("Renamed {old} to {}", new.trim())),
                Err(e) => a.status = e.to_string(),
            }
        });
    }

    /// 만든 SQL 을 보여 주고 (운영이면 빨간 창) 실행, 끝나면 트리를 다시 읽는다
    fn confirm_ddl(&mut self, target: String, conn: String, db: Option<String>, sql: Vec<String>, done: String) {
        let prod = self.production_name(&conn);
        let text = format!(
            "{}\n\n{}\n\nRun it?",
            match &prod {
                Some(p) => format!("Production data source \"{p}\" — this changes the schema:"),
                None => "Run this?".into(),
            },
            sql.iter().map(|s| format!("  {}", s.split_whitespace().collect::<Vec<_>>().join(" "))).collect::<Vec<_>>().join("\n")
        );
        self.popup = Some(Popup::Confirm {
            text,
            on_yes: Box::new(move |a| {
                let st = a.state.clone();
                let db = db.unwrap_or_else(|| a.default_db(&conn));
                a.spawn(async move { st.run_ddl(target, sql).await }, move |a, r| match r {
                    Ok(()) => {
                        // 스키마 · 테이블은 그 DB 아래를, 새 DB 는 Data Source 를 다시 읽는다
                        a.schema_changed(&conn, &db);
                        if let Some(n) = a.tree.roots.iter().find(|n| n.conn == conn).cloned() {
                            a.refresh_node(&n);
                        }
                        a.status = done;
                    }
                    Err(e) => a.status = e.to_string(),
                });
            }),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_lines() {
        let (c, pk) = parse_column("email character varying(100) not null default 'a b' comment 'it''s'").unwrap();
        assert_eq!((c.name.as_str(), c.data_type.as_str(), c.nullable, c.default.as_deref(), c.comment.as_deref(), pk), ("email", "character varying(100)", false, Some("'a b'"), Some("it's"), false));
        let (c, pk) = parse_column("id bigint auto pk").unwrap();
        assert!(pk && c.auto_increment && !c.nullable);
        let (c, _) = parse_column("price numeric(10, 2) default 0").unwrap();
        assert_eq!((c.data_type.as_str(), c.default.as_deref(), c.nullable), ("numeric(10, 2)", Some("0"), true));
        assert!(parse_column("x").is_err());
        // 타입은 여러 낱말일 수 있다 (double precision), 모르는 키워드 짝은 오류
        assert_eq!(parse_column("x double precision").unwrap().0.data_type, "double precision");
        assert!(parse_column("x int not wat").unwrap_err().contains("not"));
        // 글로 바꿨다가 다시 읽으면 같다
        let (c, pk) = parse_column("status text not null default 'active' comment 'state'").unwrap();
        assert_eq!(parse_column(&column_spec(&c, pk)).unwrap(), (c, pk));
    }

    #[test]
    fn index_and_fk_lines() {
        let i = parse_index("users_email_idx (email, org_id) unique").unwrap();
        assert_eq!((i.columns.clone(), i.unique), (vec!["email".to_string(), "org_id".to_string()], true));
        assert_eq!(index_spec(&i), "users_email_idx (email, org_id) unique");
        assert!(parse_index("x email").is_err());
        let f = parse_fk("orders_user_fk (user_id) -> crm.users (id) on delete set null on update cascade", "public").unwrap();
        assert_eq!((f.ref_schema.as_str(), f.ref_table.as_str(), f.on_delete.as_str(), f.on_update.as_str()), ("crm", "users", "SET NULL", "CASCADE"));
        assert_eq!(fk_spec(&f, "public"), "orders_user_fk (user_id) -> crm.users (id) on delete set null on update cascade");
        let g = parse_fk("fk (a) -> t (id)", "public").unwrap();
        assert_eq!((g.ref_schema.as_str(), g.on_delete.as_str()), ("public", "NO ACTION"));
        assert!(parse_fk("fk (a) t (id)", "public").is_err());
    }
}
