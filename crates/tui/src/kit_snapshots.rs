//! Storybook "TUI" screens: scenes drawn by the real TUI code on a virtual terminal, saved as JSON with characters and colors intact.
//! Regenerate: `pnpm tui-snapshots` (= cargo test -p qurry-tui kit_snapshots -- --ignored)
//! → src/ui/stories/tui-snapshots.json. `snapshots_match_storybook` fails when the drawn scenes differ from that file
use crate::app::Popup;
use crate::kit::*;
use crate::testkit::Harness;
use crate::tree::{Node, NodeKind};
use qurry_core::store::ConnectionConfig;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use serde_json::{json, Value};

/// Terminal color → CSS (a common dark terminal palette, or a common light one for scenes in the light theme)
fn css(c: Color, light: bool) -> Value {
    if light {
        let s = match c {
            Color::Black => "#383a42",
            Color::Red => "#e45649",
            Color::Green => "#50a14f",
            Color::Yellow => "#c18401",
            Color::Blue => "#4078f2",
            Color::Magenta => "#a626a4",
            Color::Cyan => "#0184bc",
            Color::Gray => "#dcdcdc",
            Color::DarkGray => "#6e6e6e",
            Color::White => "#f2f2f2",
            Color::LightRed => "#ca1243",
            Color::LightGreen => "#3d8c3c",
            Color::LightYellow => "#986801",
            Color::LightBlue => "#2f5fd0",
            Color::LightMagenta => "#8e2a8c",
            Color::LightCyan => "#0b7aa3",
            _ => return css(c, false),
        };
        return json!(s);
    }
    let s = match c {
        Color::Reset => return Value::Null,
        Color::Rgb(r, g, b) => return json!(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Black => "#1e1f22",
        Color::Red => "#e06c75",
        Color::Green => "#98c379",
        Color::Yellow => "#e5c07b",
        Color::Blue => "#61afef",
        Color::Magenta => "#c678dd",
        Color::Cyan => "#56b6c2",
        Color::Gray => "#abb2bf",
        Color::DarkGray => "#6b7079",
        Color::White => "#e6e6e6",
        Color::LightRed => "#f07178",
        Color::LightGreen => "#b5e48c",
        Color::LightYellow => "#ffd580",
        Color::LightBlue => "#82aaff",
        Color::LightMagenta => "#d7a1f9",
        Color::LightCyan => "#89ddff",
        // xterm 256 palette (0–15 are the named colors above; the TUI doesn't use them)
        Color::Indexed(n @ 16..) => {
            let (r, g, b) = crate::theme::rgb256(n);
            return json!(format!("#{r:02x}{g:02x}{b:02x}"));
        }
        Color::Indexed(_) => "#abb2bf",
    };
    json!(s)
}

/// Buffer → per line, runs of [text, fg, bg, bold, underline, italic, strikethrough, wide] (same styles are merged), plus [reverse, dim]
/// at the end only when one of them is on (NO_COLOR draws with them).
/// Wide (two-cell) characters (Korean) get their own run (wide = true) — so the web draws them exactly two cells wide. The following cell is skipped
fn dump(buf: &Buffer, light: bool) -> Value {
    use unicode_width::UnicodeWidthStr;
    let a = buf.area;
    let mut rows = vec![];
    for y in a.top()..a.bottom() {
        let mut runs: Vec<(String, Value, Value, u16, bool)> = vec![];
        let mut x = a.left();
        while x < a.right() {
            let c = &buf[(x, y)];
            let w = c.symbol().width().max(1) as u16;
            let (fg, bg, m) = (css(c.fg, light), css(c.bg, light), c.modifier.bits());
            let wide = w > 1;
            match runs.last_mut() {
                Some(r) if !wide && !r.4 && r.1 == fg && r.2 == bg && r.3 == m => r.0.push_str(c.symbol()),
                _ => runs.push((c.symbol().to_string(), fg, bg, m, wide)),
            }
            x += w;
        }
        let b = |m: u16, f: Modifier| m & f.bits() != 0;
        rows.push(Value::Array(
            runs.into_iter()
                .map(|(t, fg, bg, m, wide)| {
                    let mut run = json!([t, fg, bg, b(m, Modifier::BOLD), b(m, Modifier::UNDERLINED), b(m, Modifier::ITALIC), b(m, Modifier::CROSSED_OUT), wide]);
                    let (rev, dim) = (b(m, Modifier::REVERSED), b(m, Modifier::DIM));
                    if rev || dim {
                        run.as_array_mut().unwrap().extend([json!(rev), json!(dim)]);
                    }
                    run
                })
                .collect(),
        ));
    }
    json!({ "w": a.width, "h": a.height, "rows": rows })
}

fn scene(name: &str, desc: &str, w: u16, h: u16, draw: impl FnOnce(&mut Frame)) -> Value {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(draw).unwrap();
    let mut v = dump(term.backend().buffer(), false);
    v["name"] = json!(name);
    v["desc"] = json!(desc);
    v
}

/// The app's screen; in the light theme it is marked light (Storybook draws it on a light terminal)
fn app_scene(h: &mut Harness, name: &str, desc: &str, w: u16, ht: u16) -> Value {
    let mut term = Terminal::new(TestBackend::new(w, ht)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
    let light = crate::theme::Look::of(&h.app.themes, h.app.auto_light).light;
    let mut v = dump(term.backend().buffer(), light);
    v["name"] = json!(name);
    v["desc"] = json!(desc);
    if light {
        v["light"] = json!(true);
    }
    v
}

/// Atoms: color tokens · text pieces (badge · key · muted text · icon · color swatch)
fn atoms() -> Value {
    scene("Atoms", "kit/atoms.rs: 색 토큰과 글자 조각 — badge · key · muted · icon · swatch", 110, 12, |f| {
        let toks = [("ACCENT", ACCENT), ("SEL", SEL), ("SEL_BLUR", SEL_BLUR), ("SURFACE", SURFACE), ("POPUP_BG", POPUP_BG), ("OK", OK), ("WARN", WARN), ("DANGER", DANGER), ("NUMBER", NUMBER), ("C_DB", C_DB), ("C_VIEW", C_VIEW), ("C_SEQUENCE", C_SEQUENCE), ("C_ROUTINE", C_ROUTINE), ("C_KEYWORD", C_KEYWORD)];
        let mut lines = vec![Line::from(muted("color tokens (GUI tokens.css 와 같은 값)"))];
        for chunk in toks.chunks(5) {
            lines.push(Line::from(chunk.iter().flat_map(|(n, c)| [swatch(*c), Span::raw(" "), muted(format!("{n:<12}"))]).collect::<Vec<_>>()));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![muted("badge   "), badge("PROD", DANGER), Span::raw(" "), badge("READ-ONLY", SEL), Span::raw(" "), badge("STRING", Color::Rgb(95, 158, 160)), Span::raw(" "), badge("v0.2.5 available", WARN)]));
        lines.push(Line::from(vec![muted("key     "), key("⌃k"), Span::raw(" "), key("F5"), Span::raw(" "), key("⌥⇧N")]));
        lines.push(Line::from(vec![muted("icon    "), icon("◉", C_DB), icon("⛁", C_DB), icon("◇", Color::Gray), icon("▤", ACCENT), icon("▦", Color::Gray), icon("◫", C_VIEW), icon("#", C_SEQUENCE), icon("ƒ", C_ROUTINE)]));
        lines.push(Line::from(vec![muted("muted   "), muted("설명 · 개수 · 위치")]));
        f.render_widget(Paragraph::new(lines), f.area());
    })
}

/// Molecules: one line — Row (tree · palette · schema picker · Redis key), key_row, field_row, key_hints, section
fn molecules() -> Value {
    scene("Molecules", "kit/molecules.rs: 한 줄 조각 — Row (들여쓰기 · ▸ · 아이콘 · 설명 · 오른쪽 · 색 · 선택), key_row, field_row, key_hints, section", 110, 22, |f| {
        let w = 60;
        let prod = Some(Color::Rgb(0x6e, 0x2a, 0x2a));
        let lines = vec![
            Line::from(muted("Row — 트리 (정확히 60칸, 줄 색 끝까지 · 끝에 걸린 한글은 빈칸)")),
            Row::new("[PROD] shop").chevron(Some(true)).lead(icon("◉", C_DB)).bold().hint("postgres").bg(prod).fit(0, w),
            Row::new("tables").indent(2).chevron(Some(true)).lead(icon("▤", ACCENT)).hint("66").bg(prod).fit(0, w),
            Row::new("adjust").indent(3).chevron(Some(false)).lead(icon("▦", Color::Gray)).hint("정산 테이블 (주문 단위 정산) 매우 긴 설명").bg(prod).fit(0, w),
            Row::new("payment").indent(3).chevron(Some(false)).lead(icon("▦", Color::Gray)).hint("결제 카드 정보").bg(prod).selected(true, true).fit(0, w),
            Row::new("orders").indent(3).chevron(Some(false)).lead(icon("▦", Color::Gray)).selected(true, false).fit(0, w),
            Line::raw(""),
            Line::from(muted("Row — 팔레트 · 스키마 고르기 · Redis 키 (오른쪽 맞춤)")),
            Row::new("Database Admin…").hint(" sessions & locks, slow queries").selected(true, true).fit(0, w),
            Row::new("public").indent(1).chevron(None).lead(muted("[x] ")).bold().hint("(Default schema)").fit(0, w),
            Row::new("session:abc").chevron(None).lead(badge("STRING", Color::Rgb(95, 158, 160))).lead(Span::raw(" ")).trailing("3540s").fit(0, w),
            Line::raw(""),
            section("section(\"Commands\")"),
            key_row("⌃k", "Actions Palette", 20, false, false),
            key_row("⌥n", "New Workspace…", 20, true, true),
            Line::from(field_row("Host", 15, "db.example.com", false)),
            Line::from(field_row("Workspace", 15, "◂ 1 Default ▸", true)),
            Line::raw(""),
            key_hints(&[("y / Enter", "yes"), ("n / Esc", "no"), ("Tab", "next")]),
        ];
        f.render_widget(Paragraph::new(lines), f.area());
    })
}

/// Organisms: panel (focused · not) · popup · danger popup (confirmation)
fn organisms() -> Value {
    scene("Organisms", "kit/organisms.rs: panel(포커스 · 아님) · popup · popup_danger + confirm_lines (행 수 색)", 120, 16, |f| {
        let [a, b, c] = Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(30), Constraint::Percentage(42)]).areas(f.area());
        f.render_widget(Paragraph::new(vec![Line::raw("panel(title, true)"), Line::from(muted("굵은 강조 테두리"))]).block(panel(" Focused ", true)), Rect { height: 6, ..a });
        f.render_widget(Paragraph::new(vec![Line::raw("panel(title, false)"), Line::from(muted("흐린 얇은 테두리"))]).block(panel(" Not focused ", false)), Rect { y: a.y + 7, height: 6, ..a });
        let inner = popup(f, Rect { x: b.x + 1, width: b.width - 2, height: 7, ..b }, " Confirm ");
        f.render_widget(Paragraph::new(confirm_lines("Delete snippet \"user orders\"?")), inner);
        let text = "Production data source \"[PROD] kr\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n  delete from payment where created_at < now() - interval '1 year'\n     → 12,408 rows\n\nRun it?";
        let lines = confirm_lines(text);
        let h = wrapped_height(&lines, c.width - 4) + 2;
        let inner = popup_danger(f, Rect { x: c.x + 1, width: c.width - 2, height: h, ..c }, " Confirm ");
        f.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), inner);
    })
}

fn conn(id: &str, name: &str, color: &str, prod: bool) -> ConnectionConfig {
    serde_json::from_value(json!({
        "id": id, "name": name, "kind": "postgres", "host": "db.example.com", "port": 5432, "user": "app",
        "database": "shop", "color": color, "options": { "confirmChanges": prod }
    }))
    .unwrap()
}

fn node(key: &str, label: &str, hint: &str, kind: NodeKind, conn: &str) -> Node {
    Node { key: key.into(), label: label.into(), hint: hint.into(), kind, conn: conn.into(), db: Some("shop".into()), schema: Some("public".into()), name: None, expandable: true }
}

/// Explorer (production · development colors, Korean descriptions) + table tab + status line (PROD badge)
async fn explorer(h: &mut Harness) {
    for c in [conn("prod", "[PROD] shop", "#6e2a2a", true), conn("dev", "[DEV] shop", "#2f5634", false)] {
        h.app.state.save_connection(c, None, None).await.unwrap();
    }
    h.app.reload_conns();
    h.settle().await;
    let p = "prod";
    h.app.tree.children.insert(p.into(), vec![node("prod|shop", "shop", "default", NodeKind::Db, p)]);
    h.app.tree.children.insert("prod|shop".into(), vec![node("prod|shop|public", "public", "", NodeKind::Schema, p)]);
    h.app.tree.children.insert("prod|shop|public".into(), vec![node("prod|shop|public|g", "tables", "66", NodeKind::Group, p)]);
    let tables = [("adjust", "정산 테이블 (주문 단위 정산)"), ("adjust_status_log", "정산 상태 로그"), ("admin_auth", "관리자 OTP 인증"), ("admin_role", ""), ("bank_info", "은행 정보 테이블"), ("contract", "월세 계약 테이블"), ("payment", "결제 카드 정보")];
    h.app.tree.children.insert("prod|shop|public|g".into(), tables.iter().map(|(t, c)| Node { expandable: true, name: Some(t.to_string()), ..node(&format!("prod|shop|public|g|{t}"), t, c, NodeKind::Object("table".into()), p) }).collect());
    for k in ["prod", "prod|shop", "prod|shop|public", "prod|shop|public|g"] {
        h.app.tree.expanded.insert(k.into());
    }
    h.app.tree.sel = 5;
    let id = h.table_tab("payment", &["id", "user_id", "amount", "card_brand", "created_at"], &[&["1", "41", "129000", "VISA", "2026-10-28 09:12"], &["2", "7", "59000", "MASTER", "2026-10-28 09:40"], &["3", "18", "1200000", "AMEX", "2026-10-28 10:02"], &["4", "41", "0", "", "2026-10-28 11:15"]], &["id"]);
    if let Some(t) = h.app.tabs.iter_mut().find(|t| t.id == id) {
        t.conn = "prod".into();
    }
    h.app.focus = crate::app::Focus::Tree;
    h.app.status = "4 rows retrieved in 12 ms".into();
}

/// Screens: console · results, chart, production log, table designer, test data · copy related rows confirmation, Redis
fn screens(h: &mut Harness, scenes: &mut Vec<Value>) {
    use qurry_core::ddl::{ColumnDef, ForeignKeyDef, IndexDef, TableDef};
    // Console + results
    let id = h.console_with("select card_brand, count(*) as payments, sum(amount) as total\nfrom payment\ngroup by 1\norder by 3 desc;", 3, 16);
    let res = qurry_core::db::QueryResult {
        columns: [("card_brand", "TEXT"), ("payments", "INT8"), ("total", "NUMERIC")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: [("VISA", 1240, "182400000"), ("MASTER", 860, "120900000"), ("AMEX", 210, "61500000"), ("JCB", 95, "9800000")].iter().map(|(b, n, t)| vec![json!(b), json!(n), json!(t)]).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 18,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.results.ran(10, Some(0), None, String::new(), "payment".into(), crate::grid::Grid::new(res));
    }
    h.app.focus = crate::app::Focus::Editor;
    h.app.status = "4 row(s) retrieved in 18 ms".into();
    scenes.push(app_scene(h, "Console & result", "콘솔 (위) · 결과 표 (아래) · F5 실행", 120, 26));
    // Chart
    h.app.chart_result();
    scenes.push(app_scene(h, "Chart", "⌃k › Chart Result: 첫 글 컬럼별 수 컬럼의 합 (가로 막대)", 120, 26));
    h.app.popup = None;
    // Loading more (scrolled near the end of an unfinished result): the status line shows it as it shows a run
    if let Some(g) = h.app.grid_mut() {
        (g.result.truncated, g.loading_more) = (true, true);
    }
    scenes.push(app_scene(h, "Grid: loading more", "Scrolled near the end of an unfinished result: the status line shows the bar, \"Loading more rows…\" and the elapsed time, as for a run; the Cancel key stops it. Nothing covers the rows", 120, 26));
    h.app.color = crate::theme::ColorMode::NoColor;
    scenes.push(app_scene(h, "Grid: loading more, NO_COLOR", "NO_COLOR=1: the same bar and words", 120, 26));
    h.app.color = crate::theme::ColorMode::TrueColor;
    // Loading more failed (#157): the reason wraps on the grid's last lines with the key that runs the result's statement again
    let why = "Couldn't load more rows (F5 in the grid runs it again): The data source reconnected since this result was read (settings saved, disconnected or idle), and the session was changed since (SET, RESET, USE or a statement that isn't a read), so a new connection wouldn't match. Run the query again to load more rows.";
    if let Some(g) = h.app.grid_mut() {
        (g.loading_more, g.more_error) = (false, Some(why.into()));
    }
    let status = std::mem::replace(&mut h.app.status, why.into());
    h.app.focus = crate::app::Focus::Grid;
    scenes.push(app_scene(h, "Console: load more failed", "A load of more rows that failed keeps its reason on the grid's last lines, wrapped (here at 80 columns), with how to go on: the run key in the grid runs the result's statement again, not the editor's", 80, 26));
    (h.app.focus, h.app.status) = (crate::app::Focus::Editor, status);
    if let Some(g) = h.app.grid_mut() {
        (g.result.truncated, g.more_error) = (false, None);
    }
    // Horizontal scrollbar on the grid's bottom border (#107): 14 columns, 3–8 shown
    let id = h.console_with("select * from payment;", 0, 22);
    let names = ["idx", "order_no", "user_idx", "payment_type", "tx_type", "sub_type", "contract_idx", "merchant_idx", "partner_idx", "sales_idx", "receiver_name", "receiver_contact", "receiver_bank", "created_at"];
    let wide = qurry_core::db::QueryResult {
        columns: names.iter().enumerate().map(|(i, n)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: if i % 4 == 0 { "INT8" } else { "TEXT" }.into() }).collect(),
        rows: (0..12).map(|r| names.iter().enumerate().map(|(i, _)| if i % 4 == 0 { json!(486 + r * 3 + i) } else { json!(format!("{}-{}", ["CARD", "BANK", "PAY", "NORMAL"][(r + i) % 4], 100 + r)) }).collect()).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 12,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        let mut g = crate::grid::Grid::new(wide);
        (g.row, g.col, g.col_off) = (2, 4, 2);
        c.results.ran(10, Some(0), None, String::new(), "payment".into(), g);
    }
    let status = std::mem::replace(&mut h.app.status, "12 row(s) retrieved in 12 ms".into());
    use crate::theme::ColorMode;
    const BAR: &str = "the bottom border is the bar: a dim thin track, the thumb heavier and bold over the shown columns, the shown range at its right end; click or drag the track";
    for (focus, color, theme, name, desc) in [
        (crate::app::Focus::Grid, ColorMode::TrueColor, None, "Grid scrollbar: focused", format!("A result with more columns than fit: {BAR}. Focused: the thick cyan border, the thumb and range in cyan")),
        (crate::app::Focus::Editor, ColorMode::TrueColor, None, "Grid scrollbar: not focused", format!("The same thumb when the grid isn't focused; only the border's color and shape change ({BAR})")),
        (crate::app::Focus::Grid, ColorMode::TrueColor, Some("light"), "Grid scrollbar: light", "Theme Light: the same bar on a light terminal".into()),
        (crate::app::Focus::Grid, ColorMode::Ansi16, None, "Grid scrollbar: 16 colors", "TERM=xterm: the thumb cyan and bold, the track dark gray".into()),
        (crate::app::Focus::Grid, ColorMode::NoColor, None, "Grid scrollbar: NO_COLOR", "NO_COLOR=1: the thumb by its heavier bold line (━ over ─)".into()),
    ] {
        (h.app.focus, h.app.color, h.app.themes.theme) = (focus, color, theme.map(String::from));
        scenes.push(app_scene(h, name, &desc, 120, 22));
    }
    (h.app.color, h.app.themes.theme) = (ColorMode::TrueColor, None);
    if let Some(g) = h.app.grid_mut() {
        (g.result.truncated, g.more_error) = (true, Some("Couldn't load more rows (F5 in the grid runs it again): Session closed".into()));
    }
    scenes.push(app_scene(h, "Grid scrollbar: with a failed load-more", "The load-more reason sits on the grid's last line, above the bottom edge, which keeps the whole bar and its range", 120, 22));
    h.app.focus = crate::app::Focus::Editor;
    h.app.close_tab_now(h.app.active);
    h.app.status = status;
    // Long Output log: the newest line sits on the bottom row, a long wrapped one included
    let id = h.console_with("update payment set memo = 'migrated' where id = 1;", 0, 0);
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        for i in 1..=40 {
            c.log(false, format!("shop> update payment set memo = 'migrated' where id = {i}"));
            c.log(false, "1 row(s) affected in 2 ms".into());
        }
        c.log(false, format!("shop> insert into payment_audit (payment_id, memo, payload) values (41, 'migrated', '{{{}}}')", (1..=12).map(|k| format!("\"field_{k}\": \"value {k}\"")).collect::<Vec<_>>().join(", ")));
        c.log(true, "ERROR: value too long for type character varying(255)".into());
    }
    scenes.push(app_scene(h, "Console: long Output", "A script's Output longer than the pane: the newest line sits on the bottom row, wrapped long lines included", 120, 22));
    h.app.close_tab_now(h.app.active);
    // Console result from one table with its key: edited like a table tab (one pending change), then one it can't edit and why
    let id = h.console_with("select id, user_name as name, email from users;", 0, 10);
    let users = |editable| qurry_core::db::QueryResult {
        columns: [("id", "INT8"), ("name", "VARCHAR"), ("email", "TEXT")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: [(1, "kim", "kim@example.com"), (2, "lee", "lee@example.com"), (3, "park", "park@example.com")].iter().map(|(i, n, e)| vec![json!(i), json!(n), json!(e)]).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 3,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: Some(editable),
        notices: vec![],
    };
    let target = qurry_core::editable::EditTarget { schema: "public".into(), table: "users".into(), pk: vec!["id".into()], columns: vec!["id".into(), "user_name".into(), "email".into()] };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        let mut g = crate::grid::Grid::new(users(qurry_core::editable::Editable::Table(target)));
        g.set_value(1, 1, "Lee".into());
        (g.row, g.col) = (1, 1);
        c.results.ran(10, Some(0), None, String::new(), "users".into(), g);
    }
    h.app.focus = crate::app::Focus::Grid;
    h.app.status = "3 row(s) retrieved in 3 ms".into();
    scenes.push(app_scene(h, "Console result: editable", "A console result from one table with its primary key edits like a table tab: ↩ edit, a add, d delete, ⌃s submit (Production asks first), ⌃z revert", 120, 22));
    // A join (#102): each table's cells by its own key; the outer side where no row matched says why
    {
        use qurry_core::editable::{ColumnEdit, Editable, JoinEdit, JoinTable};
        let jt = |s: &str, t: &str, at: usize| JoinTable { schema: s.into(), table: t.into(), pk: vec!["idx".into()], pk_at: vec![at] };
        let to = |t: usize, c: &str| ColumnEdit::To { table: t, column: c.into() };
        let j = JoinEdit { tables: vec![jt("crm", "voc", 0), jt("public", "users", 2)], columns: vec![to(0, "idx"), to(0, "status"), to(1, "idx"), to(1, "role")] };
        let joined = qurry_core::db::QueryResult {
            columns: [("idx", "INT8"), ("status", "VARCHAR"), ("idx", "INT8"), ("role", "VARCHAR")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
            rows: vec![vec![json!(1), json!("open"), json!(10), json!("member")], vec![json!(2), json!("new"), json!(null), json!(null)], vec![json!(3), json!("closed"), json!(11), json!("admin")]],
            editable: Some(Editable::Join(j)),
            ..users(Editable::ReadOnly(String::new()))
        };
        if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
            let mut g = crate::grid::Grid::new(joined);
            g.set_value(0, 3, "admin".into());
            (g.row, g.col) = (1, 3);
            c.results.ran(10, Some(0), None, String::new(), "crm.voc".into(), g);
        }
    }
    h.key("Enter");
    scenes.push(app_scene(h, "Console result: join", "A join edits per table, each by its own primary key (users.role of voc 1 edited); a cell where no row of its table matched (outer join) is read-only, and the status line says why", 120, 22));
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.results.ran(10, Some(0), None, String::new(), "users".into(), crate::grid::Grid::new(users(qurry_core::editable::Editable::ReadOnly(qurry_shared::errors::edit_missing_pk("public.users", &["id".into()])))));
    }
    h.key("a");
    scenes.push(app_scene(h, "Console result: read-only", "An edit key on a result Qurry can't tie to table rows: the status line says why (here the key isn't in the result)", 120, 22));
    // A Production console starts in Manual (#106): its change is pending, and the reminder about a transaction left open
    let reminder = qurry_shared::errors::tx_idle_reminder("[PROD] shop", 5, true, Some((false, 600)));
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        (c.tx_manual, c.in_tx) = (true, true);
        c.results.show(None);
        for (e, l) in [(false, qurry_shared::errors::PRODUCTION_MANUAL.to_string()), (false, "> update users set role = 'admin' where idx = 10".into()), (false, "1 row(s) affected in 4 ms".into()), (false, reminder.clone())] {
            c.log(e, l);
        }
    }
    h.app.focus = crate::app::Focus::Editor;
    h.app.status = reminder;
    scenes.push(app_scene(h, "Console: Production transaction open", "A Production console starts in Tx: Manual and says so once: a change stays pending (the title says how to commit or roll back) and a transaction left unused for 5 minutes is reminded about, with the server's own limit", 120, 22));
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        (c.tx_manual, c.in_tx) = (false, false);
    }
    h.app.focus = crate::app::Focus::Editor;
    // Gutter marks: ✓ done, ✗ failed, … running (all three in one run here, as in the GUI story)
    let id = h.console_with("select 1;\nupdate payment\n   set status = 'paid'\n where id = 42;\nselect pg_sleep(5);", 4, 0);
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        use crate::app::MarkState;
        let lines = c.editor.lines().to_vec();
        c.marks.start(&lines, &[(0, 0), (1, 3), (4, 4)]);
        c.marks.set(&lines, 0, MarkState::Ok);
        c.marks.set(&lines, 1, MarkState::Err);
        c.marks.set(&lines, 2, MarkState::Running);
        for (e, l) in [(false, "> select 1"), (false, "0 row(s) retrieved in 3 ms"), (false, "> update payment set status = 'paid' where id = 42"), (true, "ERROR: column \"status\" of relation \"payment\" does not exist"), (false, "> select pg_sleep(5)")] {
            c.log(e, l.into());
        }
        c.running = true;
    }
    h.app.status = "Executing…".into();
    scenes.push(app_scene(h, "Console gutter marks", "Run statements are marked on their first line: ✓ done, ✗ failed, … running (click: Output)", 120, 26));
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.running = false;
    }
    // F5 inside one of several statements: the Execute chooser, the rows of the selected option highlighted in the editor
    h.console_with("select 1;\nselect 2;\nselect 3;", 1, 0);
    h.app.status.clear();
    h.key("F5");
    h.key("Down");
    h.key("Down");
    scenes.push(app_scene(h, "Console: Execute chooser", "F5 inside one of several statements asks what to run; the editor highlights the text the selected option runs (↑↓ · Enter · Esc closes without running)", 120, 22));
    h.key("Esc");
    h.app.close_tab_now(h.app.active);
    // Completion from the shared engine: FK joins, the signature hint line, unknown names underlined with their fixes (⌥↩)
    let meta = || {
        use qurry_core::complete::{CompletionTable, ForeignKey, Meta};
        let t = |n: &str, cols: &[(&str, &str)]| CompletionTable { schema: "public".into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect(), ..Default::default() };
        let mut payment = t("payment", &[("id", "int8"), ("user_id", "int8"), ("amount", "numeric"), ("card_brand", "text"), ("status", "text")]);
        payment.fks = vec![ForeignKey { columns: vec!["user_id".into()], ref_schema: "public".into(), ref_table: "users".into(), ref_columns: vec!["id".into()] }];
        std::sync::Arc::new(Meta::new(vec![payment, t("users", &[("id", "int8"), ("name", "text"), ("email", "text")]), t("audit_log", &[("id", "int8")])]))
    };
    let target = |h: &Harness| h.app.tabs[h.app.active].target.clone();
    h.console_with("select p.amount, u.name\nfrom payment p\njoin users u ", 2, 13);
    h.app.completion.insert(target(h), meta());
    h.app.focus = crate::app::Focus::Editor;
    h.key("Ctrl+Space");
    scenes.push(app_scene(h, "Console: FK join", "After join <table>, ⌃Space offers the whole ON condition from the foreign keys (both ways); after join, the tables with a foreign key to the statement's come first", 120, 22));
    h.key("Esc");
    h.app.close_tab_now(h.app.active);
    h.console_with("select card_brand,\n       coalesce(amount, ", 1, 24);
    h.app.completion.insert(target(h), meta());
    h.key("Right");
    scenes.push(app_scene(h, "Console: signature hint", "Inside a call's parentheses the hint line above the caret shows the function's signature, the parameter at the caret highlighted", 120, 22));
    h.app.close_tab_now(h.app.active);
    h.console_with("select p.amount, p.card_brnd\nfrom payment p\nwhere statsu = 'paid'", 0, 25);
    h.app.completion.insert(target(h), meta());
    h.key("End");
    scenes.push(app_scene(h, "Console: unknown name", "A table or column the statement's scope lacks is underlined (none for CTEs, aliases, functions, unloaded databases); on it the hint line names the closest names and ⌥↩ lists them", 120, 22));
    h.app.close_tab_now(h.app.active);
    // #150: the console's SQL in the theme's colors (qurry_shared::highlight of the whole text: a comment or $$ body spans lines)
    let mixed = "-- Paid card payments per brand\nselect card_brand, count(*) as payments, round(avg(amount), 2)\n  from payment\n where status = 'paid' and amount > 1000 /* KRW */\n group by 1 order by 2 desc;\n/* refunds are\n   counted apart */\ncreate function refunds() returns bigint as $$\n  select count(*) from refund;\n$$ language sql;";
    for (theme, name) in [(None, "Console: SQL colors"), (Some("light"), "Console: SQL colors (light)")] {
        h.app.themes.theme = theme.map(String::from);
        h.console_with(mixed, 4, 26);
        scenes.push(app_scene(h, name, "Keywords, functions, numbers, strings and comments in the theme's colors, as in the GUI console and the table filter; a block comment or a $$ body keeps its color on every line. Selection, search matches, the caret and error marks are drawn over them", 120, 32));
        h.app.close_tab_now(h.app.active);
    }
    h.app.themes.theme = None;
    // Quotes and brackets: typed pairs close themselves; one left open is marked with its line number, the border says what
    h.console_with("select user_id, count(*\n  from payment\n where card_brand in ('VISA', 'MASTER')\n group by 1;", 0, 23);
    scenes.push(app_scene(h, "Console: unclosed bracket", "' \" ( [ close themselves as you type (a closing one types over, ⌫ in an empty pair removes both, around a selection they wrap it); one left open is underlined in red with its line number, and on its line the bottom border says what is open", 120, 22));
    h.app.close_tab_now(h.app.active);
    // F5 inside a subquery: offered after the whole statement, innermost first; the editor highlights exactly its text
    h.console_with("select 1;\nselect * from orders\nwhere user_id in (select id from users where n > (select avg(n) from users));", 2, 58);
    h.key("F5");
    h.key("Down");
    scenes.push(app_scene(h, "Console: run a subquery", "F5 inside a subquery offers it after the whole statement, innermost first (DataGrip); the editor highlights exactly what it runs", 120, 22));
    h.key("Esc");
    h.app.close_tab_now(h.app.active);
    // A failed run's error underlined where the server points (the status line says where; an edit or the next run clears it)
    let id = h.console_with("select 1;\nselect id, n\n  from no_such_table\n where n > 1;", 2, 0);
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        use crate::app::MarkState;
        let lines = c.editor.lines().to_vec();
        c.marks.start(&lines, &[(0, 0), (1, 3)]);
        c.marks.set(&lines, 0, MarkState::Ok);
        c.marks.set(&lines, 1, MarkState::Err);
        c.run_error = Some(crate::app::RunError { span: ((2, 7), (2, 20)), lines });
        for (e, l) in [(false, "> select 1"), (false, "1 row(s) retrieved in 3 ms"), (false, "> select id, n from no_such_table where n > 1"), (true, "ERROR: relation \"no_such_table\" does not exist (position 21)")] {
            c.log(e, l.into());
        }
    }
    h.app.status = "Error at line 3, col 8: ERROR: relation \"no_such_table\" does not exist — fix it and run again".into();
    scenes.push(app_scene(h, "Console: run error", "A failed statement's error is underlined where the server points (else the whole statement); the status line says where, Output keeps it, an edit or the next run clears it", 120, 22));
    h.app.status.clear();
    h.app.close_tab_now(h.app.active);
    // Grid alignment (GUI Settings › Appearance › Data grid › Alignment, read from settings.json)
    let id = h.console_with("select * from payment_check;", 1, 10);
    let res = qurry_core::db::QueryResult {
        columns: [("card_brand", "TEXT"), ("amount", "NUMERIC"), ("refunded", "BOOL"), ("paid_at", "TIMESTAMPTZ"), ("meta", "JSONB")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: vec![
            vec![json!("VISA"), json!("129000"), json!(false), json!("2026-10-28 09:12:00 +09:00"), json!("{\"pg\":\"toss\"}")],
            vec![json!("MASTER"), Value::Null, json!(true), json!("2026-10-28 09:40:00 +09:00"), Value::Null],
            vec![Value::Null, json!("1200000"), Value::Null, Value::Null, json!("{}")],
        ],
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 4,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.results.ran(10, Some(0), None, String::new(), "payment_check".into(), crate::grid::Grid::new(res));
    }
    scenes.push(app_scene(h, "Grid alignment", "Numbers and NULL on the right by default (a NULL stays in line with the numbers); text, dates, booleans and JSON on the left", 120, 22));
    // Value panel (v): the cell's whole value beside the grid, jsonb pretty-printed and coloured (the GUI's Value Editor panel)
    h.app.focus = crate::app::Focus::Grid;
    if let Some(g) = h.app.grid_mut() {
        g.col = 4;
    }
    h.app.value_panel = true;
    scenes.push(app_scene(h, "Value panel", "v in a grid: the current cell's whole value beside the grid (follows the cursor), JSON pretty-printed; v / ⌃w close it", 120, 22));
    h.app.value_panel = false;
    if let Some(g) = h.app.grid_mut() {
        (g.col, g.col_off) = (0, 0);
    }
    h.app.focus = crate::app::Focus::Editor;
    {
        use qurry_core::format::{Align, ValueClass};
        h.app.state.store.set_grid_align(ValueClass::Null, Align::Left).unwrap();
        h.app.align = h.app.state.store.set_grid_align(ValueClass::Boolean, Align::Center).unwrap();
    }
    scenes.push(app_scene(h, "Grid alignment: changed", "settings.json \"gridAlign\": { \"null\": \"left\", \"boolean\": \"center\" } (GUI Settings › Appearance › Data grid › Alignment); the other classes keep their default", 120, 22));
    h.app.palette_grid_align(qurry_core::format::ValueClass::Null);
    scenes.push(app_scene(h, "Grid alignment palette", "⌃k › Grid Alignment…: where each kind of value sits, shared with the GUI; Enter picks Left / Center / Right (NULL also Column type)", 120, 22));
    h.key("Enter");
    scenes.push(app_scene(h, "Grid alignment: pick", "⌃k › Grid Alignment… › NULL: opens on where it sits now (Enter keeps it)", 120, 22));
    h.app.popup = None;
    {
        use qurry_core::format::{Align, ValueClass};
        h.app.state.store.set_grid_align(ValueClass::Null, Align::Column).unwrap();
        h.app.align = h.app.state.store.set_grid_align(ValueClass::Boolean, Align::Left).unwrap();
    }
    h.app.close_tab_now(h.app.active);
    // Column widths (#109): < > = on the current column, saved per table in settings.json "columnWidths" (shared with the GUI)
    // The Hangul memo is the widest (2 cells a character), though the English one has more characters
    let memo = "first order after the promotion; call before delivery";
    let wide = "배송 전에 꼭 전화 주세요. 부재 시 경비실에 맡겨 주세요";
    h.table_tab("orders", &["idx", "order_no", "memo", "created_at"], &[&["486", "O-20261101", memo, "2026-11-01"], &["487", "O-20261102", "gift wrap", "2026-11-02"], &["488", "O-20261103", wide, "2026-11-03"]], &["idx"]);
    h.app.focus = crate::app::Focus::Grid;
    if let Some(g) = h.app.grid_mut() {
        g.col = 2;
        g.set_width(2, 16);
    }
    scenes.push(app_scene(h, "Column widths", "memo (the cursor's column) 16 cells wide and cut; the key bar shows < > = width", 120, 12));
    for _ in 0..3 {
        h.key(">");
    }
    scenes.push(app_scene(h, "Column widths: wider", "> three times: each widens the current column by 4 cells (< narrows it); saved for this table, the GUI shows it too", 120, 12));
    h.key("=");
    scenes.push(app_scene(h, "Column widths: fit", "=: the column fits its widest loaded value (a Hangul character takes 2 cells, as the GUI counts it); ⌃k › Fit All Columns / Reset Column Widths do every column", 120, 12));
    h.app.run_action(crate::app::Action::ColumnReset);
    h.app.status.clear();
    h.app.close_tab_now(h.app.active);
    h.app.focus = crate::app::Focus::Editor;
    // JSON colors (the GUI's tokenizer, qurry_shared::format::json_spans): one line per cell, only what fits is read
    let id = h.console_with("select id, payload from webhook_event;", 1, 10);
    let big = format!("{{\"items\":[{}]}}", (0..3000).map(|i| format!("{{\"sku\":\"A-{i}\",\"qty\":{i}}}")).collect::<Vec<_>>().join(","));
    let res = qurry_core::db::QueryResult {
        columns: [("id", "INT8"), ("payload", "JSONB"), ("raw", "JSON"), ("sig", "BYTEA")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: vec![
            vec![json!(1), json!("{\"type\":\"payment.paid\",\"amount\":129000,\"live\":true,\"meta\":{\"pg\":\"toss\",\"coupon\":null}}"), json!("{\n  \"ok\": true\n}"), json!("0x7B22")],
            vec![json!(2), json!("{\"type\":\"refund\",\"memo\":\"고객 요청 🙏\",\"lines\":[1,2.5,-3e2]}"), json!("[\"a\", \"b\"]"), Value::Null],
            vec![json!(3), json!(big), json!("{\"broken\": tru"), Value::Null],
            vec![json!(4), Value::Null, json!("42"), Value::Null],
        ],
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 6,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.results.ran(10, Some(0), None, String::new(), "webhook_event".into(), crate::grid::Grid::new(res));
    }
    scenes.push(app_scene(h, "Grid: JSON", "json / jsonb cells in colors (keys · strings · numbers · true/false/null · punctuation), still one line; a large value reads only what fits; binary (bytea) stays plain", 120, 22));
    h.app.color = crate::theme::ColorMode::Ansi16;
    scenes.push(app_scene(h, "Grid: JSON, 16 colors", "TERM=xterm: the JSON colors by hue (NO_COLOR: no colors)", 120, 22));
    h.app.color = crate::theme::ColorMode::TrueColor;
    h.app.themes.theme = Some("light".into());
    scenes.push(app_scene(h, "Light: grid JSON", "Theme Light: the JSON colors of the GUI's light theme", 120, 22));
    h.app.themes.theme = None;
    h.app.close_tab_now(h.app.active);
    // Production log
    let entry = |id: &str, ago: i64, sql: &str, rows: Option<u64>, undo: bool, in_tx: bool| qurry_core::audit::AuditEntry {
        id: id.into(),
        ts: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64 - ago * 60_000,
        conn_id: "prod".into(),
        conn_name: "[PROD] shop".into(),
        database: None,
        sql: sql.into(),
        rows,
        error: None,
        in_tx,
        undo: undo.then(|| "UPDATE …".into()),
        note: None,
        started: false,
    };
    for e in [
        entry("1", 90, "delete from payment where created_at < now() - interval '1 year'", Some(12408), false, false),
        entry("2", 12, "update payment set amount = 0 where id = 4", Some(1), true, true),
        entry("3", 10, "COMMIT", None, false, false),
        // A transaction a CALL ended (committed or rolled back: unknown)
        qurry_core::audit::AuditEntry { note: Some(qurry_core::audit::ENDED_UNKNOWN_NOTE.into()), ..entry("6", 6, qurry_core::audit::ENDED_UNKNOWN, None, false, false) },
        // Logged before it ran, no result yet (still running, or Qurry stopped)
        qurry_core::audit::AuditEntry { started: true, ..entry("4", 1, "update payment set status = 'refunded' where id = 7", None, false, false) },
        // A COMMIT cut off by a lost connection (it may have committed)
        qurry_core::audit::AuditEntry { error: Some("Connection reset by peer (os error 54)".into()), note: Some(qurry_core::audit::COMMIT_UNKNOWN_NOTE.into()), ..entry("5", 0, qurry_core::audit::COMMIT_UNKNOWN, None, false, false) },
    ] {
        h.app.state.audit.add(e).unwrap();
    }
    h.app.production_log();
    scenes.push(app_scene(h, "Production log", "⌃k › Production Log: 운영에서 실행한 변경 (↶ undo 가 있으면 ↩ 로 새 콘솔에)", 120, 22));
    // An entry without undo SQL: ↩ shows its note over the log, wrapped (Esc goes back to the same entry)
    h.key("Enter");
    scenes.push(app_scene(h, "Production log · note", "Production Log › ↩ on COMMIT outcome unknown: 메모를 줄바꿈해 로그 위에 (Esc 로 같은 항목에)", 70, 22));
    h.app.popup = None;
    // Table designer (modify)
    let col = |n: &str, t: &str, null: bool| ColumnDef { name: n.into(), orig: Some(n.into()), data_type: t.into(), nullable: null, ..Default::default() };
    let before = TableDef {
        schema: "public".into(),
        name: "payment".into(),
        columns: vec![ColumnDef { auto_increment: true, ..col("id", "bigint", false) }, col("user_id", "bigint", false), col("amount", "numeric(12,0)", false), col("card_brand", "varchar(20)", true)],
        primary_key: vec!["id".into()],
        primary_key_name: Some("payment_pkey".into()),
        indexes: vec![IndexDef { name: "payment_user_idx".into(), orig: Some("payment_user_idx".into()), columns: vec!["user_id".into()], ..Default::default() }],
        foreign_keys: vec![ForeignKeyDef { name: "payment_user_fk".into(), orig: Some("payment_user_fk".into()), columns: vec!["user_id".into()], ref_schema: "public".into(), ref_table: "users".into(), ref_columns: vec!["id".into()], on_delete: "CASCADE".into(), on_update: "NO ACTION".into() }],
        comment: Some("결제".into()),
    };
    let mut def = before.clone();
    def.columns[3].name = "brand".into();
    def.columns.push(ColumnDef { name: "paid_at".into(), data_type: "timestamptz".into(), nullable: false, default: Some("now()".into()), ..Default::default() });
    let mut d = crate::designer::Designer { target: "prod".into(), conn: "prod".into(), kind: qurry_core::store::DbKind::Postgres, before: Some(before), def, sel: 4, sql: Ok(vec![]), message: None };
    d.refresh();
    h.app.designer = Some(d);
    scenes.push(app_scene(h, "Table designer", "⌃F6 Modify Table: 컬럼 · 인덱스 · FK 를 한 줄씩 고치면 아래에 ALTER 가 바로 보인다", 120, 34));
    h.app.designer = None;
    // Test data · copy related rows confirmation
    h.app.popup = Some(Popup::Confirm {
        opened: None,
        sql: None,
        text: "Insert 1,000 generated rows into public.payment?\n\n  id          left to the database (auto / generated)\n  user_id     picked from public.users (id)\n  amount      random values\n  card_brand  one of: VISA, MASTER, AMEX\n  created_at  random values\n\n⚠ Production data source \"[PROD] shop\"\n\nAll rows go in one transaction — if one fails, nothing is inserted.".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(h, "Generate test data", "⌃k › Generate Test Data: 컬럼마다 채우는 법을 보여 주고 확인", 120, 24));
    h.app.popup = Some(Popup::Confirm {
        opened: None,
        sql: None,
        text: "Copy 29 rows into \"[DEV] shop\"? (in insert order)\n\n  users         1  start · masked: email, name, phone\n  contract      3  child of users\n  payment      25  child of contract\n\nOne transaction; rows already there are skipped. The tables must exist in the target.".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(h, "Copy related rows", "⌃k › Copy Related Rows To: FK 로 이어진 행 · 가린 개인정보 컬럼을 보여 주고 확인", 120, 22));
    // Import Data Sources: the locations found (a fake home with DataGrip, IntelliJ and DBeaver settings), then the confirm
    h.app.import_home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/import/home").canonicalize().unwrap();
    h.app.ds_import_start();
    scenes.push(app_scene(h, "Import Data Sources — pick", "⌃k › Import Data Sources…: the DataGrip / IntelliJ / DBeaver locations found, newest IDE version first, with their counts; File or folder… takes a path", 120, 22));
    h.app.popup = None;
    h.app.ds_import_location(h.app.import_home.join("Library/Application Support/JetBrains/DataGrip2025.2/options").display().to_string());
    if let Some(Popup::ImportPick(p)) = &mut h.app.popup {
        p.move_by(2);
        p.toggle();
    }
    scenes.push(app_scene(h, "Import Data Sources — confirm", "One row per source: what is added and mapped (SSH, SSL, Read-only), password on connect or a warning, skipped ones with the reason; Space leaves a row out, y / Enter imports the checked ones", 120, 40));
    h.app.popup = None;
    // Explain Analyze of several statements that need a look: each with its own reason, danger border
    let asks: Vec<(String, String)> = ["select nextval('payment_id_seq')", "delete from payment"]
        .iter()
        .map(|s| (s.to_string(), qurry_core::guard::explain_analyze_ask(s, qurry_core::store::DbKind::Postgres).unwrap()))
        .collect();
    h.app.popup = Some(Popup::Confirm { opened: None, sql: Some(qurry_core::store::DbKind::Postgres), text: crate::app::explain_analyze_ask_text(&asks), on_yes: Box::new(|_| {}) });
    scenes.push(app_scene(h, "Explain Analyze confirm", "Explain Analyze of statements a rollback can't fully undo: each statement with its own reason", 120, 22));
    // TLS opt-in: a dev source's connect error that needs TLS (Production gets only a status line). It never replaces an open popup
    h.app.popup = None;
    h.app.offer_tls_opt_in("dev", &qurry_shared::errors::needs_tls_pg(), |_| {});
    scenes.push(app_scene(h, "Allow unencrypted", "TLS 가 없는 원격 서버에 연결 실패: 개발 Data Source 는 이 Data Source 만 암호화 없이 허용할지 묻는다 (운영은 상태 줄 안내만)", 120, 22));
    h.app.popup = None;
    // Database Admin: a refresh that ran into the time limit keeps the last sessions · a report the server can't run
    let sessions = qurry_core::db::QueryResult {
        columns: [("pid", "INT4"), ("user", "TEXT"), ("state", "TEXT"), ("blocking", "INT4"), ("query_sec", "TEXT"), ("query", "TEXT")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: [(4123, "app", "idle in transaction", 2, "184.2", "UPDATE payment SET amount = 0 WHERE id = 4"), (4188, "app", "active", 0, "61.0", "UPDATE payment SET card_brand = 'VISA' WHERE id = 4"), (4201, "batch", "active", 0, "12.4", "DELETE FROM payment WHERE id = 4")]
            .iter()
            .map(|(p, u, st, b, s, q)| vec![json!(p), json!(u), json!(st), json!(b), json!(s), json!(q)])
            .collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 9,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    };
    let timeout = "The report ran into its time limit (30 s, or a stricter one set on the server). The server may be busy or the statistics very large: try again later.\n(error returned from database: canceling statement due to statement timeout)";
    let info = |kind, id| qurry_core::dba::reports(kind).into_iter().find(|r| r.id == id);
    let d = crate::app::DbaTab { report: "sessions".into(), info: info(qurry_core::store::DbKind::Postgres, "sessions"), grid: Some(crate::grid::Grid::new(sessions)), loaded: Some(std::time::Instant::now()), error: Some(timeout.into()), running: false, run: 0, auto: true, gone: None };
    h.app.push_tab("prod", "shop".into(), "Sessions".into(), crate::app::TabKind::Dba(d));
    h.app.focus = crate::app::Focus::Grid;
    scenes.push(app_scene(h, "Database Admin: refresh failed", "A refresh that ran into the time limit keeps the last sessions (c / K still work) and says how old they are", 120, 22));
    let maria = "performance_schema / sys schema is not available on this server — enable performance_schema to see this (MariaDB has it off by default, and some of these tables only exist on MySQL).\n(error returned from database: 1146 (42S02): Table 'performance_schema.events_statements_summary_by_digest' doesn't exist)";
    let d = crate::app::DbaTab { report: "slow_queries".into(), info: info(qurry_core::store::DbKind::Mysql, "slow_queries"), grid: None, loaded: None, error: Some(maria.into()), running: false, run: 0, auto: true, gone: None };
    h.app.push_tab("prod", "shop".into(), "Slow Queries".into(), crate::app::TabKind::Dba(d));
    scenes.push(app_scene(h, "Database Admin: report error", "A report the server can't run: what to turn on, then the server's error", 120, 22));
    // d: the database the report runs on (the grid's title names it)
    let id = h.app.tabs[h.app.active].id;
    h.app.dba_db_choices(id, ["postgres", "shop", "shop_archive"].map(String::from).to_vec());
    scenes.push(app_scene(h, "Database Admin: database", "d: every database of the server, the current one first and selected; picking one reruns the report there", 120, 22));
    h.app.popup = None;
    for _ in 0..2 {
        h.app.close_tab_now(h.app.active);
    }
    // DDL → Structure (v)
    let text = "CREATE TABLE public.users (\n    id bigint NOT NULL\n);".to_string();
    let t = crate::app::TextTab { text, ddl_of: Some(("public".into(), "users".into())), kind: "table".into(), structure_view: true, structure: Some(Ok(crate::structure::sample())), ..Default::default() };
    h.app.push_tab("prod", "shop".into(), "users [DDL]".into(), crate::app::TabKind::Text(t));
    h.app.focus = crate::app::Focus::Grid;
    scenes.push(app_scene(h, "DDL: Structure", "A table's DDL tab, v: columns, keys, indexes, foreign keys both ways, triggers (not reported by this source), options and comment; v again for the SQL", 120, 40));
    h.app.close_tab_now(h.app.active);
    // Table filter completion (w)
    let id = h.table_tab("payment", &["id", "user_id", "amount", "card_brand", "created_at"], &[&["1", "41", "129000", "VISA", "2026-10-28 09:12"], &["2", "7", "59000", "MASTER", "2026-10-28 09:40"]], &["id"]);
    h.app.focus = crate::app::Focus::Grid;
    h.app.input_filter(id, false);
    h.type_str("user_id = 41 and ca");
    scenes.push(app_scene(h, "Table filter completion", "w / o on a table tab: the WHERE / ORDER BY input completes like the console (the table's columns first, then keywords); ↑↓ PgUp PgDn move, Enter / Tab pick, Esc closes the list first", 120, 22));
    h.app.popup = None;
    // #136: the inputs draw their SQL in the console's colors (qurry_shared::highlight, the GUI's fields too)
    let colored = "lower(card_brand) LIKE 'vi%' AND user_id IN (41, 7) AND amount IS NOT NULL -- recent";
    for (theme, name) in [("dark", "Table filter: SQL colors"), ("light", "Table filter: SQL colors (light)")] {
        h.app.themes.theme = Some(theme.into());
        h.app.input_filter(id, false);
        h.type_str(colored);
        for _ in 0..12 {
            h.key("Left");
        }
        scenes.push(app_scene(h, name, "w: keywords (word operators IN, IS, NOT, NULL, AND, LIKE too), functions, numbers, strings and comments in the console's colors; =, < and names in the text color. The caret mid-text keeps its look", 120, 22));
        h.app.popup = None;
    }
    h.app.themes.theme = None;
    h.app.input_filter(id, true);
    h.type_str("created_at DESC NULLS LAST, id");
    scenes.push(app_scene(h, "Table filter: ORDER BY colors", "o: DESC, NULLS, LAST in the keyword color", 120, 22));
    h.app.popup = None;
    h.app.close_tab_now(h.app.active);
}

/// Redis (#108): the explorer lists a source's databases with key counts (no keys); one tab per source, the database (d) and filter
/// (/) in its title; folder counts beside the name, TTLs only for keys; the command line (:) with completion and argument hints
async fn redis(h: &mut Harness, scenes: &mut Vec<Value>) {
    use qurry_core::redisdb::{KeyRow, RedisDb};
    let cfg = serde_json::from_value(json!({ "id": "rd", "name": "[DEV] shop redis", "kind": "redis", "host": "cache.example.com", "port": 6379, "user": "", "database": "0", "color": "#2f5634" })).unwrap();
    h.app.state.save_connection(cfg, None, None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    let dbs = [("db0", 500), ("db1", 12), ("db2", 0)];
    let db_node = |d: &str, n: i64| Node { key: format!("rd|{d}"), label: d.into(), hint: n.to_string(), kind: NodeKind::Db, conn: "rd".into(), db: Some(d.into()), schema: None, name: None, expandable: false };
    h.app.tree.children.insert("rd".into(), dbs.iter().map(|(d, n)| db_node(d, *n)).collect());
    h.app.tree.expanded.insert("rd".into());
    let keys = [
        ("dev:admin_sessions:{17}", "zset", -1),
        ("dev:sessions:{405}", "zset", -1),
        ("prod:refresh:{9}", "string", 86_400_000),
        ("prod:user:mkt-daily-signup:2026-05-05", "string", 101_000_000),
        ("prod:user:mkt-daily-signup:2026-05-16", "string", 101_000_000),
        ("queue:mail", "list", -1),
    ];
    let id = h.redis_tab(&[]);
    let t = h.app.tabs.iter_mut().find(|t| t.id == id).unwrap();
    (t.conn, t.title, t.target) = ("rd".into(), "[DEV] shop redis [db0]".into(), "rd::db0".into());
    if let crate::app::TabKind::Redis(r) = &mut t.kind {
        r.keys = keys.iter().map(|(k, ty, ttl)| KeyRow { key: k.to_string(), ty: ty.to_string(), ttl: *ttl }).collect();
        r.dbs = dbs.iter().map(|(d, n)| RedisDb { name: d.to_string(), keys: *n }).collect();
        r.open.extend(["prod".to_string(), "prod:user".into(), "prod:user:mkt-daily-signup".into()]);
        r.sel = 5;
    }
    h.app.tree.sel = h.app.tree.visible().iter().position(|(_, n)| n.key == "rd|db0").unwrap_or(0);
    h.app.focus = crate::app::Focus::Grid;
    scenes.push(app_scene(h, "Redis", "Explorer: a Redis source's databases with key counts, no keys. One tab per source: the database (d) and key filter (/) in the title; folder counts beside the name, the TTL column only for keys (— without expiry)", 120, 22));
    h.app.redis_db_choices(id, dbs.iter().map(|(d, n)| RedisDb { name: d.to_string(), keys: *n }).collect());
    scenes.push(app_scene(h, "Redis: database", "d: the server's databases with their key counts, the current one selected; picking one shows its keys in the same tab (Enter or a click on a database in the explorer does too)", 120, 22));
    h.app.popup = None;
    h.app.redis_prompt(id);
    h.type_str("HG");
    scenes.push(app_scene(h, "Redis: command line", ": runs a command: completion with arguments from the bundled command table (keys from a bounded SCAN), ↑↓ history, HELP <command>; read-only runs reads only, Production asks before writes, FLUSHDB / DEL always ask", 120, 22));
    h.key("Tab");
    scenes.push(app_scene(h, "Redis: argument hints", "After the command: the arguments still to type, muted, after the text", 120, 22));
    // Key names as a bounded SCAN reads them for the typed word (set here: nothing connects)
    h.app.popup = None;
    h.app.redis_prompt(id);
    if let Some(Popup::Input { input, redis: Some(p), complete, .. }) = &mut h.app.popup {
        input.insert_str("GET user:41");
        p.keys = Some(("user:41".into(), vec!["user:41:cart".into(), "user:41".into(), "user:41 (old)".into()]));
        *complete = crate::app::App::redis_complete(input, p).0;
    }
    scenes.push(app_scene(h, "Redis: key names", "A key argument: names from a bounded SCAN, the key typed out first; ↩ runs the line unless ↑↓ moved to another key, ⇥ picks; a key with spaces goes in quoted", 120, 22));
    h.app.popup = None;
    h.app.close_tab_now(h.app.active);
    h.app.tree.children.remove("rd");
    h.app.state.delete_connection("rd".into()).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    // The later scenes as before (a key browser tab open, nothing in the status line)
    h.redis_tab(&["session:9f2c", "session:a771", "user:41", "user:41:cart", "queue:mail", "rank:daily"]);
    h.app.status.clear();
    h.app.focus = crate::app::Focus::Tree;
}

/// Every Storybook scene, in file order
async fn all_scenes() -> Vec<Value> {
    // Same text on every OS: the snapshots hold the macOS key notation (⌃k), which Linux CI would otherwise draw as Ctrl+k
    crate::keys::FORCE_MAC_GLYPHS.set(true);
    let mut scenes = vec![atoms(), molecules(), organisms()];
    let mut h = Harness::new().await;
    explorer(&mut h).await;
    scenes.push(app_scene(&mut h, "Explorer & table", "탐색기 (운영 색 · 왼쪽 끝까지 이어지는 줄 색 · 한글 설명), 표 탭, 상태 줄의 PROD 배지", 120, 22));
    // The same screen in each color mode (theme.rs maps the drawn frame; the mode comes from NO_COLOR · COLORTERM · TERM)
    use crate::theme::ColorMode;
    for (mode, name, desc) in [
        (ColorMode::TrueColor, "Colors: truecolor", "COLORTERM=truecolor: the theme's 24-bit colors as they are"),
        (ColorMode::Ansi256, "Colors: 256", "TERM=xterm-256color: each color mapped to the nearest of the xterm 256 palette"),
        (ColorMode::Ansi16, "Colors: 16", "TERM=xterm: the 16 terminal colors by hue; text on light backgrounds turns dark"),
        (ColorMode::NoColor, "Colors: NO_COLOR", "NO_COLOR=1: no colors; selections and badges in reverse video, muted text dim, the focused pane by its thick border"),
    ] {
        h.app.color = mode;
        scenes.push(app_scene(&mut h, name, desc, 120, 22));
    }
    // The light theme (⌃k › Theme…, or Sync with OS on a terminal whose COLORFGBG says its background is light) in each color mode
    h.app.themes.theme = Some("light".into());
    for (mode, name, desc) in [
        (ColorMode::TrueColor, "Light: truecolor", "Theme Light (or Sync with OS with COLORFGBG=0;15): the light palette's 24-bit colors; a Data Source color becomes a light tint of its hue"),
        (ColorMode::Ansi256, "Light: 256", "Theme Light, TERM=xterm-256color: the light palette mapped to the nearest of the xterm 256 palette"),
        (ColorMode::Ansi16, "Light: 16", "Theme Light, TERM=xterm: light surfaces, text in the normal (darker) of the 16 colors"),
        (ColorMode::NoColor, "Light: NO_COLOR", "Theme Light, NO_COLOR=1: the same monochrome rules as in the dark theme"),
    ] {
        h.app.color = mode;
        scenes.push(app_scene(&mut h, name, desc, 120, 22));
    }
    // Each built-in theme (the GUI's, qurry_shared::theme), truecolor: Dark, Light and High contrast on the terminal's background,
    // the others on their own
    h.app.color = ColorMode::TrueColor;
    for t in qurry_core::theme::table().themes.iter() {
        h.app.themes.theme = Some(t.id.clone());
        let bg = if ["dark", "light", "hc"].contains(&t.id.as_str()) { "on the terminal's own background" } else { "on its own background" };
        scenes.push(app_scene(&mut h, &format!("Theme: {}", t.name), &format!("⌃k › Theme… › {}: the theme's colors, {bg}", t.name), 120, 22));
    }
    h.app.color = ColorMode::NoColor;
    h.app.themes.theme = None;
    // Still NO_COLOR: what doesn't depend on color in the grid
    let (focus, g) = (h.app.focus, h.app.grid_mut().unwrap());
    let at = (g.row, g.col);
    g.set_value(1, 3, "61000".into());
    g.deleted.insert(2);
    g.anchor = Some((0, 1));
    (g.row, g.col) = (1, 2);
    h.app.focus = crate::app::Focus::Grid;
    scenes.push(app_scene(&mut h, "Colors: NO_COLOR grid", "NO_COLOR=1: unsaved changes by the row number (~ edited · - deleted · + added) and bold cells; the cursor is the plain cell in the reversed range", 120, 22));
    (h.app.color, h.app.themes.theme) = (ColorMode::TrueColor, Some("light".into()));
    scenes.push(app_scene(&mut h, "Light: grid edits", "Theme Light: an edited cell, a deleted row and a range with the cursor in it", 120, 22));
    (h.app.color, h.app.themes.theme) = (ColorMode::NoColor, None);
    let g = h.app.grid_mut().unwrap();
    g.edits.clear();
    g.deleted.clear();
    g.anchor = None;
    (g.row, g.col) = at;
    h.app.focus = focus;
    h.app.color = ColorMode::TrueColor;
    // Marked rows (m): rows 1 and 4 selected whole, the cursor on row 3
    h.app.focus = crate::app::Focus::Grid;
    h.key("m");
    for _ in 0..3 {
        h.key("Down");
    }
    h.key("m");
    h.key("Up");
    scenes.push(app_scene(&mut h, "Grid: marked rows", "m adds the current row to the selection or takes it out (the GUI's ⌘-click on a row number): marked rows look selected, their numbers reversed; y copies and d deletes them all, Esc clears them", 120, 22));
    // Status line items (#132): the marked rows are the selection, over the cursor's column; then one cell's Size; the checklist
    {
        use qurry_core::statusbar::{ColumnType, Item};
        for i in [Item::NotNull, Item::Sum, Item::Avg, Item::Min, Item::Max] {
            h.app.status_items = h.app.state.store.set_status_bar_item(i, true).unwrap();
        }
        h.key("Right");
        h.key("Right");
        scenes.push(app_scene(&mut h, "Status line: selection items", "Not null, Sum, Avg, Min and Max turned on (⌃k › Status Bar Items…), cursor on amount: over the marked rows' amount; Position says which rows and the column", 120, 22));
        h.key("Esc");
        h.key("Right");
        let target = h.app.tabs[h.app.active].target.clone();
        h.app.table_cols.insert((target.clone(), "public".into(), "payment".into()), crate::app::TableCols::Loaded(vec![("card_brand".into(), ColumnType { data_type: "character varying(16)".into(), charset: None })], vec![]));
        h.app.status_items = h.app.state.store.set_status_bar_item(Item::Size, true).unwrap();
        h.app.status = "4 rows".into();
        scenes.push(app_scene(&mut h, "Status line: Size", "Size turned on, nothing marked: Position is the cursor's cell, Size its bytes and the column's declared limit (from the table's metadata)", 120, 22));
        scenes.push(app_scene(&mut h, "Status line: narrow", "80 columns, every item on: the screen's key hints go first, then the global ones but ⌃k actions, then the items (the limit, Max, Min …); Position stays", 80, 22));
        h.app.palette_status_items(0);
        scenes.push(app_scene(&mut h, "Status bar items checklist", "⌃k › Status Bar Items…: the GUI's status bar checklist (shared through settings.json); Enter or Space toggles and it stays open, the line below follows", 120, 22));
        h.app.popup = None;
        h.app.table_cols.clear();
        h.app.status_items = h.app.state.store.reset_status_bar_items().unwrap();
        h.key("Left");
        h.key("Left");
        h.key("Left");
    }
    h.key("Esc");
    let g = h.app.grid_mut().unwrap();
    (g.row, g.col) = at;
    h.app.focus = focus;
    scenes.push(app_scene(&mut h, "Terminal too small", "Below 40×10 the layout isn't drawn: the size, the size needed and how to quit", 36, 9));
    // Marked nodes (m, the GUI's ⌘-click), then Delete on them: one confirmation for all, the Production one on a Production source
    for t in ["adjust", "contract", "payment"] {
        h.app.tree.select_key(&format!("prod|shop|public|g|{t}"));
        h.app.run_action(crate::app::Action::MarkNode);
    }
    scenes.push(app_scene(&mut h, "Explorer: marked nodes", "m marks nodes (the GUI's ⌘-click): Go to DDL, F4, y, Delete and refresh act on every marked node; m unmarks, Esc clears", 120, 22));
    h.app.run_action(crate::app::Action::DropObject);
    h.settle().await;
    scenes.push(app_scene(&mut h, "Explorer: drop marked objects", "Delete on marked objects of one database: one confirmation lists every statement (red on a Production source, as for one object)", 120, 22));
    h.app.popup = None;
    h.app.tree.marked.clear();
    h.app.tree.sel = 5;
    h.app.status = "4 rows retrieved in 12 ms".into();
    h.app.popup = Some(Popup::Confirm {
        opened: None,
        sql: Some(qurry_core::store::DbKind::Postgres),
        text: "Production data source \"[PROD] shop\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n  delete from payment where created_at < now() - interval '1 year'\n     → 12,408 rows\n\nRun it?".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(&mut h, "Production confirm", "운영 Data Source 에서 바꾸는 문장: 바뀔 행 수를 보여 주고 묻는다", 120, 22));
    h.app.themes.theme = Some("light".into());
    scenes.push(app_scene(&mut h, "Light: production confirm", "Theme Light: the Production question, the red row and the PROD badge on a light terminal", 120, 22));
    h.app.themes.theme = None;
    scenes.push(app_scene(&mut h, "Terminal too small: question waiting", "Below 40×10 with a dialog open, ⌃q doesn't quit: the message says to make the window larger to answer it", 36, 9));
    scenes.push(app_scene(&mut h, "Production confirm: minimum size", "At 40×10 the question and its keys stay; the statements that don't fit are counted", 40, 10));
    // Question first, warnings after the details: the columns / tables in between give way, the ⚠ lines stay
    let cols: Vec<_> = (0..12).map(|i| qurry_core::fake::ColPlan { name: format!("col_{i}"), data_type: "text".into(), how: "random values".into() }).collect();
    h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text: crate::datatools::fake_confirm_text(1000, "public.payment", &cols, &[], Some("[PROD] shop")), on_yes: Box::new(|_| {}) });
    scenes.push(app_scene(&mut h, "Generate test data: minimum size", "At 40×10 the question, the ⚠ Production line and the keys stay; the columns in between are counted as hidden", 40, 10));
    let tables = (0..24)
        .map(|i| qurry_core::subset::SubsetTable { schema: "public".into(), table: format!("contract_{i}"), rows: 25, masked: vec![], reason: "child of users".into() })
        .collect();
    let plan = qurry_core::subset::SubsetPlan { tables, total: 600, warning: None };
    h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text: crate::datatools::copy_confirm_text(&plan, "[PROD] shop", Some("[PROD] shop"), false), on_yes: Box::new(|_| {}) });
    scenes.push(app_scene(&mut h, "Copy related rows: 80×24", "Into a Production source, not masked, with more tables than fit: both ⚠ warnings and the question stay, the tables that don't fit are counted", 80, 24));
    let notes: String = (0..8).map(|i| format!("\n⚠ Column note_{i} has a check constraint the generated values may not meet")).collect();
    h.app.popup = Some(Popup::Confirm { opened: None, sql: None, text: format!("Insert 10 generated rows into public.notes?\n{notes}"), on_yes: Box::new(|_| {}) });
    scenes.push(app_scene(&mut h, "Confirm: too small to answer", "When even the question and the ⚠ lines don't fit, y / Enter don't answer: the keys line says to enlarge the window (n / Esc still cancel)", 40, 10));
    h.app.popup = None;
    h.app.palette_actions();
    h.type_str("work");
    scenes.push(app_scene(&mut h, "Actions palette", "⌃k: 모든 명령과 단축키 (입력으로 찾기)", 120, 22));
    h.app.popup = None;
    h.app.conn_form(None);
    scenes.push(app_scene(&mut h, "Data source form", "n (탐색기): 새 Data Source. Workspace · Color 칸은 ◂ ▸ 로 고른다", 120, 34));
    h.key("Up");
    scenes.push(app_scene(&mut h, "Data source form: 80×24", "In a window too short for every field the fields scroll to the focused one (here the last); the key hints stay and the border says which field it is", 80, 24));
    h.app.popup = None;
    let ssm = serde_json::from_value(json!({ "id": "ssm", "name": "[PROD] orders (SSM)", "kind": "postgres", "host": "orders.cluster-abc.ap-northeast-2.rds.amazonaws.com", "port": 5432, "user": "app", "database": "orders",
        "ssm": { "enabled": true, "profile": "prod", "target": "i-0123456789abcdef0" } })).unwrap();
    h.app.conn_form(Some(ssm));
    scenes.push(app_scene(&mut h, "Data source form: AWS SSM", "Tunnel ◂ aws ssm ▸: AWS profile · region (empty: the profile's) · SSM target (instance id). Host / Port are the database as seen from the instance; no AWS secrets are stored (the aws CLI uses your AWS config or SSO login)", 120, 34));
    h.app.popup = None;
    // #131: whether AI tools may read a Production source's admin reports (MCP dba_report); disabled with why elsewhere
    let cases = [("off", true, "read", false, false), ("on", true, "read", true, false), ("not Production", false, "read", false, false), ("MCP Off", true, "off", false, false), ("off", true, "read", false, true), ("on", true, "read", true, true)];
    for (name, prod, mcp, on, light) in cases {
        let mut c = conn("prod", if prod { "[PROD] shop" } else { "[DEV] shop" }, if prod { "#6e2a2a" } else { "#2f5634" }, prod);
        (c.options.mcp, c.options.mcp_dba_reports) = (Some(mcp.into()), on);
        h.app.themes.theme = light.then(|| "light".into());
        h.app.conn_form(Some(c));
        if let Some(Popup::ConnForm(f)) = &mut h.app.popup {
            let at = f.fields.iter().position(|x| x.label == "Admin reports").unwrap();
            f.sel = if f.disabled("Admin reports").is_none() { at } else { at - 1 };
        }
        let l = if light { " (light)" } else { "" };
        scenes.push(app_scene(&mut h, &format!("Data source form: admin reports, {name}{l}"), "Admin reports: AI tools may read this Production source's Database Admin reports (MCP dba_report), query values masked; off by default. Not Production (always readable) or MCP Off: disabled, dim", 120, 36));
    }
    h.app.themes.theme = None;
    h.app.popup = None;
    h.app.popup = Some(Popup::Help { sel: 0, scroll: 0, record: None, message: None });
    scenes.push(app_scene(&mut h, "Keymap (F1)", "F1: 처음 쓸 키 → 바꿀 수 있는 명령 → 고정 키", 120, 30));
    // Recording a key for the explorer's c (Connect): pane keys follow every command, then the fixed keys
    let sel = crate::keys::COMMANDS.iter().position(|(s, _)| *s == crate::keys::Slot { action: crate::app::Action::Connect, pane: qurry_core::keymap::Scope::Tree }).unwrap();
    h.app.popup = Some(Popup::Help { sel, scroll: 0, record: Some(false), message: None });
    scenes.push(app_scene(&mut h, "Keymap (F1): explorer keys", "Every command can get a key, those without a default show —; the explorer's and the grid's letter commands are rebindable too (here: recording a new key for c Connect)", 120, 30));
    h.app.popup = None;
    screens(&mut h, &mut scenes);
    redis(&mut h, &mut scenes).await;
    // MySQL backup: after the path, the DEFINER= question (question on the border, answers and reason in the box below)
    let my = serde_json::from_value(json!({ "id": "my", "name": "shop", "kind": "mysql", "host": "db.example.com", "port": 3306, "user": "app", "database": "shop" })).unwrap();
    h.app.state.save_connection(my, None, None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.status.clear();
    let tab = h.app.restore_console("my", "shop".into(), "console".into(), "").unwrap();
    let i = h.app.tab_index(tab).unwrap();
    h.app.activate(i);
    h.app.focus = crate::app::Focus::Editor;
    h.app.backup_start();
    h.key("Enter");
    scenes.push(app_scene(&mut h, "MySQL backup: DEFINER= question", "⌃k › Back Up Database…: after the path, MySQL asks whether to leave out DEFINER= clauses", 80, 24));
    h.app.popup = None;
    h.app.close_tab_now(h.app.active);
    h.app.state.delete_connection("my".into()).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    h.app.focus = crate::app::Focus::Tree;
    h.key("Alt+n");
    h.key("Enter");
    h.key("Alt+1");
    scenes.push(app_scene(&mut h, "Workspaces", "워크스페이스가 둘 이상이면 위쪽 띠 (⌥1…9 · 눌러서 바꾸기)", 120, 22));
    h.app.palette_update_channel();
    scenes.push(app_scene(&mut h, "Update channel", "⌃k › Update Channel…: stable (version tags only, the default) or beta (also every merge to main), shared with the GUI", 120, 22));
    // Run from the palette; a test launches no browser (the opener is stubbed)
    h.app.popup = None;
    h.key("Ctrl+K");
    h.type_str("Open GitHub");
    h.key("Enter");
    scenes.push(app_scene(&mut h, "Open GitHub", "⌃k › Open GitHub: the project page opens in the browser; the status line shows its address for sessions where none opens (SSH)", 120, 22));
    h.app.status.clear();
    h.app.palette_theme();
    scenes.push(app_scene(&mut h, "Theme", "⌃k › Theme…: the built-in and custom themes (shared with the GUI); Sync with OS (the default) takes the GUI's dark or light pick by the terminal background (COLORFGBG)", 120, 22));
    h.app.auto_light = true;
    h.app.palette_theme();
    scenes.push(app_scene(&mut h, "Theme: Sync with OS on a light terminal", "COLORFGBG=0;15: Sync with OS draws its light theme (Dark brings the dark colors back)", 120, 22));
    h.app.auto_light = false;
    h.app.popup = None;
    tab_bars(&mut scenes).await;
    reconnect_scenes(&mut scenes).await;
    result_tab_scenes(&mut scenes).await;
    scenes.push(sequence_behind_scene().await);
    scenes.push(temporary_scene().await);
    scenes
}

/// #167: a new row's INSERT got a key its sequence had already given out; the notice says so under the server's error, with the
/// SQL to copy (Qurry doesn't run it)
async fn sequence_behind_scene() -> Value {
    fn dup(q: &str) -> Result<qurry_core::db::QueryResult, String> {
        Err(format!("error returned from database: duplicate key value violates unique constraint \"orders_pkey\"\nKey (id)=(487) already exists.\n→ {q}"))
    }
    let mut h = Harness::new().await;
    h.app.fake_db = Some(dup);
    h.table_tab("orders", &["id", "order_no", "memo"], &[&["486", "O-20261101", "gift wrap"], &["487", "O-20261102", ""]], &["id"]);
    let target = h.app.tabs[h.app.active].target.clone();
    h.app.table_cols.insert((target, "public".into(), "orders".into()), crate::app::TableCols::Loaded(vec![], vec!["id".into()]));
    h.app.focus = crate::app::Focus::Grid;
    h.key("a");
    h.key("Ctrl+S");
    h.settle().await;
    app_scene(&mut h, "Submit failed: key sequence behind", "A new row's id came from a sequence behind the table's highest id: the server's error, what happened, and the setval to copy and run", 120, 26)
}

/// #135: a table tab and an explorer node waiting on [PROD] shop while it reconnects through AWS SSM: the status line says so with
/// the time and the Cancel key (past 10 s why), a failure opens the Message popup, Cancel leaves how to load again (the GUI's toast)
async fn reconnect_scenes(scenes: &mut Vec<Value>) {
    use crate::app::{Action, TabKind, WaitFor};
    let mut h = Harness::new().await;
    explorer(&mut h).await;
    let mut ssm = conn("prod", "[PROD] shop", "#6e2a2a", true);
    ssm.ssm = Some(qurry_core::store::SsmConfig { enabled: true, profile: "prod".into(), region: String::new(), target: "i-0a1b2c3d4e5f".into() });
    h.app.state.store.save(&ssm).unwrap();
    h.app.reload_conns();
    h.settle().await;
    let st = h.app.state.clone();
    let shown = st.reconnecting_now("prod");
    let tab = h.app.tabs.iter().find(|t| t.conn == "prod").map(|t| t.id).unwrap();
    let load = |h: &mut Harness| {
        if let Some(TabKind::Table(tt)) = h.app.tabs.iter_mut().find(|t| t.id == tab).map(|t| &mut t.kind) {
            (tt.grid, tt.running) = (None, true);
        }
        h.app.spawn_wait("prod", WaitFor::Table(tab), std::future::pending::<()>(), |_, _, _| {});
    };
    // Shown since `secs` ago (drawn right after, so the time reads the same on every run)
    let since = |h: &mut Harness, secs: f64| {
        h.app.reconnect_line();
        *h.app.reconnect_since.get_mut("prod").unwrap() = std::time::Instant::now() - std::time::Duration::from_secs_f64(secs);
    };
    load(&mut h);
    (h.app.focus, h.app.status) = (crate::app::Focus::Grid, String::new());
    since(&mut h, 3.2);
    scenes.push(app_scene(&mut h, "Reconnect: table tab waiting", "A table tab opens at once and loads inside it; while its source reconnects through AWS SSM the status line says so, how long, and that the Cancel key stops it (the GUI's running indicator and toast)", 120, 22));
    since(&mut h, 14.0);
    scenes.push(app_scene(&mut h, "Reconnect: slow", "Past 10 s: why it may take a while, after the Cancel key (a narrower line cuts the note, never the key)", 160, 22));
    h.app.run_action(Action::Cancel);
    scenes.push(app_scene(&mut h, "Reconnect: cancelled", "The Cancel key stops the loads waiting on the source (the connect they wait on is dropped); the tab stays, r loads it again", 120, 22));
    let r = h.app.state.reconnecting().pop().unwrap();
    let e = format!("{}: aws ssm start-session: TargetNotConnected (i-0a1b2c3d4e5f is not connected). Check that the instance is running and its SSM agent is online.", r.failed);
    h.app.reconnect_failed(&r, &e, true);
    scenes.push(app_scene(&mut h, "Reconnect: failed", "A load that waited on the reconnect failed: the Message popup says why and that r in the table tab loads it again; the status line keeps it", 120, 22));
    h.app.popup = None;
    // The explorer: a schema expanding while the source reconnects shows only its loading mark
    let key = "prod|shop|public";
    let tables = h.app.tree.children.remove(key);
    h.app.tree.loading.insert(key.into());
    h.app.spawn_wait("prod", WaitFor::Tree(key.into()), std::future::pending::<()>(), |_, _, _| {});
    (h.app.focus, h.app.tree.sel, h.app.status) = (crate::app::Focus::Tree, 2, String::new());
    since(&mut h, 4.1);
    scenes.push(app_scene(&mut h, "Reconnect: explorer expand", "Expanding a schema while its source reconnects: the tree shows only that it loads, the status line says why", 120, 22));
    h.app.run_action(Action::Cancel);
    h.app.tree.children.insert(key.into(), tables.unwrap());
    h.app.themes.theme = Some("light".into());
    load(&mut h);
    (h.app.focus, h.app.status) = (crate::app::Focus::Grid, String::new());
    since(&mut h, 3.2);
    scenes.push(app_scene(&mut h, "Reconnect: table tab waiting (light)", "Theme Light: the same line on a light terminal", 120, 22));
    h.app.run_action(Action::Cancel);
    drop(shown);
}

/// `qurry-tui --read-only postgres://…`: the temporary source at the top of the explorer, its mark before the long URL name
async fn temporary_scene() -> Value {
    let mut h = Harness::new().await;
    explorer(&mut h).await;
    let var = |_: &str| None;
    let env = qurry_core::dsn::Env { var: &var, home: None };
    let t = qurry_core::dsn::from_url("postgres://app@db.internal.example.com:5432/shop", true, &env).unwrap();
    h.app.state.open_temporary(t);
    h.app.reload_conns();
    h.settle().await;
    h.app.tree.sel = 0;
    app_scene(&mut h, "Explorer: temporary source", "qurry-tui --read-only postgres://…: the temporary data source at the top of the explorer, TEMP · READ-ONLY before its name (a long URL is cut at the end, not the mark)", 120, 22)
}

/// Split panes with many tabs of two Data Sources in each color mode: the focused (left) pane's active [PROD] tab next to an inactive [DEV] one, the other pane's active [DEV] tab next to an inactive [PROD] one.
/// Tab glyphs follow the tree: a view's data and DDL ◫, a function's DDL ƒ
async fn tab_bars(scenes: &mut Vec<Value>) {
    use crate::theme::ColorMode;
    let mut h = Harness::new().await;
    for c in [conn("prod", "[PROD] shop", "#6e2a2a", true), conn("dev", "[DEV] shop", "#2f5634", false)] {
        h.app.state.save_connection(c, None, None).await.unwrap();
    }
    h.app.reload_conns();
    h.settle().await;
    fn tab(h: &mut Harness, name: &str, conn: &str) -> u64 {
        let id = h.table_tab(name, &["id"], &[&["1"]], &["id"]);
        h.app.tabs.iter_mut().find(|t| t.id == id).unwrap().conn = conn.into();
        id
    }
    fn ddl(h: &mut Harness, name: &str, conn: &str, kind: &str) {
        h.app.push_tab(conn, "shop".into(), format!("{name} [DDL]"), crate::app::TabKind::Text(crate::app::TextTab { text: String::new(), kind: kind.into(), ..Default::default() }));
    }
    let users = tab(&mut h, "users", "prod");
    let orders = tab(&mut h, "v_orders", "dev");
    if let Some(crate::app::TabKind::Table(t)) = h.app.tabs.iter_mut().find(|t| t.id == orders).map(|t| &mut t.kind) {
        t.kind = "view".into();
    }
    for (name, conn) in [("adjust", "dev"), ("payment", "prod")] {
        tab(&mut h, name, conn);
    }
    // The active tab when splitting moves to the new pane
    ddl(&mut h, "v_orders", "prod", "view");
    h.app.run_action(crate::app::Action::SplitRight);
    let role = tab(&mut h, "admin_role", "dev");
    ddl(&mut h, "calc_fee", "prod", "routine");
    for id in [role, users] {
        let i = h.app.tab_index(id).unwrap();
        h.app.activate(i);
    }
    h.app.focus = crate::app::Focus::Grid;
    h.app.status.clear();
    for (mode, name, desc) in [
        (ColorMode::TrueColor, "Tab bar: truecolor", "Active tab: bold on a lighter Data Source color (or the selection color) in the focused pane, underlined on its own color in the other pane"),
        (ColorMode::Ansi16, "Tab bar: 16 colors", "TERM=xterm: the focused pane's tab in reverse video + bold, keeping its Data Source color (a lighter hue is often the same terminal color); the other pane's bold + underlined"),
        (ColorMode::NoColor, "Tab bar: NO_COLOR", "NO_COLOR=1: the focused pane's tab in reverse video + bold, the other pane's bold + underlined"),
    ] {
        h.app.color = mode;
        scenes.push(app_scene(&mut h, name, desc, 120, 10));
    }
    (h.app.color, h.app.themes.theme) = (ColorMode::TrueColor, Some("light".into()));
    scenes.push(app_scene(&mut h, "Tab bar: light", "Theme Light: Data Source colors as light tints; the focused pane's active tab in the stronger hue", 120, 10));
}

/// The shop tables the result tab scenes run against (the GUI story's): `select * from <table> [where status = 'paid'] [limit n]`
fn shop_db(q: &str) -> Result<qurry_core::db::QueryResult, String> {
    let table = q.split(" from ").nth(1).and_then(|s| s.split_whitespace().next()).unwrap_or("").trim_end_matches(';');
    let (cols, rows): (&[&str], Vec<Vec<Value>>) = match table {
        "customers" => {
            let city = q.split("city = '").nth(1).and_then(|s| s.split('\'').next());
            let all = ["Kim", "Lee", "Park", "Choi", "Jung", "Kang", "Cho", "Yoon"].iter().enumerate().map(|(i, n)| (i + 1, *n, ["Seoul", "Busan", "Incheon"][i % 3]));
            (&["id", "name", "city"], all.filter(|r| city.is_none_or(|c| c == r.2)).map(|(i, n, c)| vec![json!(i), json!(n), json!(c)]).collect())
        }
        "orders" => (&["id", "customer_id", "status", "total"], (0..24).filter(|i| !q.contains("'paid'") || i % 3 == 0).map(|i| vec![json!(1000 + i), json!(i % 8 + 1), json!(["paid", "refunded", "pending"][i % 3]), json!((i * 37) % 500 + 20)]).collect()),
        "payments" | "products" => (&["id", "order_id", "method", "amount"], (0..16).map(|i| vec![json!(i + 1), json!(1000 + i), json!(["card", "bank", "point"][i % 3]), json!((i * 53) % 400 + 10)]).collect()),
        t => return Err(format!("ERROR: relation \"{t}\" does not exist")),
    };
    let limit = q.split(" limit ").nth(1).and_then(|s| s.trim().trim_end_matches(';').parse().ok()).unwrap_or(rows.len());
    Ok(qurry_core::db::QueryResult {
        columns: cols.iter().map(|n| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: if *n == "name" || *n == "city" || *n == "status" || *n == "method" { "text" } else { "int8" }.into() }).collect(),
        rows: rows.into_iter().take(limit).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 4,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
        notices: vec![],
    })
}

/// #151: a console's result tabs follow its statements; the result panel's title is their strip (Output │ 1: customers │ …)
async fn result_tab_scenes(scenes: &mut Vec<Value>) {
    use tui_textarea::CursorMove::Jump;
    let mut h = Harness::new().await;
    h.app.fake_db = Some(shop_db);
    h.app.exec.inside = qurry_core::exec::Inside::Smallest;
    let caret = |h: &mut Harness, row: u16, col: u16| {
        if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.get_mut(h.app.active).map(|t| &mut t.kind) {
            c.editor.move_cursor(Jump(row, col));
        }
    };
    let s3 = "select * from customers;\nselect * from orders where status = 'paid';\nselect * from payments limit 5;";
    h.console_with(s3, 0, 3);
    h.app.focus = crate::app::Focus::Editor;
    h.key("F5");
    h.settle().await;
    h.key("Down");
    h.key("F5");
    h.settle().await;
    scenes.push(app_scene(&mut h, "Console result tabs: run 1, then 2", "Each statement keeps its own result tab, numbered by its place in the script; the one just run is shown", 120, 22));
    h.key("Up");
    scenes.push(app_scene(&mut h, "Console result tabs: caret back on 1", "The caret in statement 1 shows its tab again, without running it", 120, 22));
    h.app.themes.theme = Some("light".into());
    scenes.push(app_scene(&mut h, "Console result tabs: caret back on 1 (light)", "The same in the light theme", 120, 22));
    h.app.themes.theme = None;
    caret(&mut h, 0, 23);
    h.type_str(" where city = 'Busan'");
    h.key("F5");
    h.settle().await;
    scenes.push(app_scene(&mut h, "Console result tabs: edit 1 and run again", "Statement 1 was edited and run again: its tab gets the new result, no new tab", 120, 22));
    h.app.run_action(crate::app::Action::PinResult);
    h.type_str(" limit 2");
    h.key("F5");
    h.settle().await;
    scenes.push(app_scene(&mut h, "Console result tabs: pinned, run again", "⌃k › Pin / Unpin Result Tab, then statement 1 run again: a new tab follows it, the pinned one (📌) stays as it was", 120, 22));
    h.app.close_tab_now(h.app.active);
    h.console_with(s3, 0, 3);
    h.app.exec.inside = qurry_core::exec::Inside::Whole;
    h.key("F5");
    h.settle().await;
    h.app.exec.inside = qurry_core::exec::Inside::Smallest;
    caret(&mut h, 1, 0);
    h.key("Shift+Down");
    h.key("Backspace");
    caret(&mut h, 0, 0);
    h.key("Home");
    scenes.push(app_scene(&mut h, "Console result tabs: statement deleted", "Statement 2 was deleted, so its tab closed; statement 3 is now 2 and its tab says so", 120, 22));
    h.app.close_tab_now(h.app.active);
    // Ten tabs, statement 1's shown again, then an eleventh: the least recently shown unpinned tab (statement 2's) closes
    let t = ["customers", "orders", "payments", "products"];
    let s11: Vec<String> = (0..11).map(|i| format!("select * from {} limit {};", t[i % 4], i + 1)).collect();
    h.console_with(&s11.join("\n"), 0, 3);
    for i in 0..11 {
        if i == 10 {
            caret(&mut h, 0, 0);
            h.key("Home");
        }
        caret(&mut h, i, 3);
        h.key("F5");
        h.settle().await;
    }
    scenes.push(app_scene(&mut h, "Console result tabs: 11th tab", "Result tabs (10): the 11th closed the least recently shown (statement 2's; statement 1's was shown just before); tabs that don't fit fold into ‹n", 120, 22));
    h.app.close_tab_now(h.app.active);
    h.console_with(s3, 0, 3);
    h.key("F5");
    h.settle().await;
    h.key("Down");
    h.key("F5");
    h.settle().await;
    caret(&mut h, 1, 20);
    h.key("Backspace");
    h.key("F5");
    h.settle().await;
    scenes.push(app_scene(&mut h, "Console result tabs: error", "Statement 2 failed after an edit: Output is shown with the error; statement 2's tab keeps its last result", 120, 22));
    h.app.palette_result_tabs();
    scenes.push(app_scene(&mut h, "Console result tabs: Result Tabs… (default)", "⌃k › Result Tabs…: unpinned result tabs kept per console (shared with the GUI's Settings › Appearance › Console › Result tabs)", 120, 22));
    h.key("Backspace");
    h.key("Backspace");
    h.type_str("0");
    h.key("Enter");
    scenes.push(app_scene(&mut h, "Console result tabs: Result Tabs… (invalid 0)", "A number that isn't 1 or more isn't saved: the box says why", 120, 22));
}

fn json_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ui/stories/tui-snapshots.json")
}

#[tokio::test]
#[ignore = "only when regenerating the TUI screens for Storybook (pnpm tui-snapshots)"]
async fn kit_snapshots() {
    let out = json_path();
    std::fs::write(&out, serde_json::to_string(&json!({ "scenes": all_scenes().await })).unwrap()).unwrap();
    println!("wrote {}", out.display());
}

/// The committed Storybook file matches what the TUI draws now, cell by cell: text and style (runs of [text, fg, bg, bold, underline, italic, strikethrough, wide])
#[tokio::test]
async fn snapshots_match_storybook() {
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(json_path()).unwrap()).unwrap();
    let saved = saved["scenes"].as_array().unwrap();
    let now = all_scenes().await;
    let names = |v: &[Value]| v.iter().map(|s| s["name"].clone()).collect::<Vec<_>>();
    assert_eq!(names(saved), names(&now), "scene list changed — regenerate: pnpm tui-snapshots");
    let text = |row: &Value| row.as_array().unwrap().iter().filter_map(|r| r[0].as_str()).collect::<String>();
    for (a, b) in saved.iter().zip(&now) {
        let name = &b["name"];
        assert_eq!((&a["w"], &a["h"], &a["desc"]), (&b["w"], &b["h"], &b["desc"]), "scene {name}: size or description changed — regenerate: pnpm tui-snapshots");
        for (y, (ra, rb)) in a["rows"].as_array().unwrap().iter().zip(b["rows"].as_array().unwrap()).enumerate() {
            assert!(ra == rb, "scene {name}, row {y} differs — regenerate: pnpm tui-snapshots\n  saved: {}\n  now:   {}\n  saved runs: {ra}\n  now runs:   {rb}", text(ra), text(rb));
        }
    }
}
