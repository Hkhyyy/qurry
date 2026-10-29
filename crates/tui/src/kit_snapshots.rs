//! Storybook 의 "TUI" 화면: 실제 TUI 코드로 가상 터미널에 그린 장면을 글자 · 색 그대로 JSON 으로 남긴다.
//! 다시 만들기: `pnpm tui-snapshots` (= cargo test -p qurry-tui kit_snapshots -- --ignored)
//! → src/ui/stories/tui-snapshots.json
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

/// 터미널 색 → CSS (흔한 다크 터미널 팔레트)
fn css(c: Color) -> Value {
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
        Color::Indexed(_) => "#abb2bf",
    };
    json!(s)
}

/// 버퍼 → 줄마다 [글자, fg, bg, 굵게, 밑줄, 기울임, 취소선, 두칸] 묶음 (같은 스타일은 합친다).
/// 두 칸 글자 (한글) 는 따로 한 묶음 (두칸 = true) — 웹에서 정확히 두 칸 폭으로 그리게. 그 뒤 칸은 건너뛴다
fn dump(buf: &Buffer) -> Value {
    use unicode_width::UnicodeWidthStr;
    let a = buf.area;
    let mut rows = vec![];
    for y in a.top()..a.bottom() {
        let mut runs: Vec<(String, Value, Value, u16, bool)> = vec![];
        let mut x = a.left();
        while x < a.right() {
            let c = &buf[(x, y)];
            let w = c.symbol().width().max(1) as u16;
            let (fg, bg, m) = (css(c.fg), css(c.bg), c.modifier.bits());
            let wide = w > 1;
            match runs.last_mut() {
                Some(r) if !wide && !r.4 && r.1 == fg && r.2 == bg && r.3 == m => r.0.push_str(c.symbol()),
                _ => runs.push((c.symbol().to_string(), fg, bg, m, wide)),
            }
            x += w;
        }
        let b = |m: u16, f: Modifier| m & f.bits() != 0;
        rows.push(Value::Array(runs.into_iter().map(|(t, fg, bg, m, wide)| json!([t, fg, bg, b(m, Modifier::BOLD), b(m, Modifier::UNDERLINED), b(m, Modifier::ITALIC), b(m, Modifier::CROSSED_OUT), wide])).collect()));
    }
    json!({ "w": a.width, "h": a.height, "rows": rows })
}

fn scene(name: &str, desc: &str, w: u16, h: u16, draw: impl FnOnce(&mut Frame)) -> Value {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(draw).unwrap();
    let mut v = dump(term.backend().buffer());
    v["name"] = json!(name);
    v["desc"] = json!(desc);
    v
}

fn app_scene(h: &mut Harness, name: &str, desc: &str, w: u16, ht: u16) -> Value {
    let mut term = Terminal::new(TestBackend::new(w, ht)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
    let mut v = dump(term.backend().buffer());
    v["name"] = json!(name);
    v["desc"] = json!(desc);
    v
}

/// 원자: 색 토큰 · 글자 조각 (배지 · 키 · 흐린 글자 · 아이콘 · 색 견본)
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

/// 분자: 한 줄 — Row (트리 · 팔레트 · 스키마 고르기 · Redis 키), key_row, field_row, key_hints, section
fn molecules() -> Value {
    scene("Molecules", "kit/molecules.rs: 한 줄 조각 — Row (들여쓰기 · ▸ · 아이콘 · 설명 · 오른쪽 · 색 · 선택), key_row, field_row, key_hints, section", 110, 22, |f| {
        let w = 60;
        let prod = Some(Color::Rgb(0x6e, 0x2a, 0x2a));
        let lines = vec![
            Line::from(muted("Row — 트리 (정확히 60칸, 줄 색 끝까지 · 끝에 걸린 한글은 빈칸)")),
            Row::new("[PROD] rentalpay").chevron(Some(true)).lead(icon("◉", C_DB)).bold().hint("postgres").bg(prod).fit(0, w),
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

/// 유기체: 패널 (포커스 · 아님) · 팝업 · 위험 팝업 (확인 창)
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
        "database": "rentalpay", "color": color, "options": { "confirmChanges": prod }
    }))
    .unwrap()
}

fn node(key: &str, label: &str, hint: &str, kind: NodeKind, conn: &str) -> Node {
    Node { key: key.into(), label: label.into(), hint: hint.into(), kind, conn: conn.into(), db: Some("rentalpay".into()), schema: Some("public".into()), name: None, expandable: true }
}

/// 탐색기 (운영 · 개발 색, 한글 설명) + 표 탭 + 상태 줄 (PROD 배지)
async fn explorer(h: &mut Harness) {
    for c in [conn("prod", "[PROD] rentalpay", "#6e2a2a", true), conn("dev", "[DEV] rentalpay", "#2f5634", false)] {
        h.app.state.save_connection(c, None, None).await.unwrap();
    }
    h.app.reload_conns();
    h.settle().await;
    let p = "prod";
    h.app.tree.children.insert(p.into(), vec![node("prod|rentalpay", "rentalpay", "default", NodeKind::Db, p)]);
    h.app.tree.children.insert("prod|rentalpay".into(), vec![node("prod|rentalpay|public", "public", "", NodeKind::Schema, p)]);
    h.app.tree.children.insert("prod|rentalpay|public".into(), vec![node("prod|rentalpay|public|g", "tables", "66", NodeKind::Group, p)]);
    let tables = [("adjust", "정산 테이블 (주문 단위 정산)"), ("adjust_status_log", "정산 상태 로그"), ("admin_auth", "관리자 OTP 인증"), ("admin_role", ""), ("bank_info", "은행 정보 테이블"), ("contract", "월세 계약 테이블"), ("payment", "결제 카드 정보")];
    h.app.tree.children.insert("prod|rentalpay|public|g".into(), tables.iter().map(|(t, c)| Node { expandable: true, ..node(&format!("prod|rentalpay|public|g|{t}"), t, c, NodeKind::Object("table".into()), p) }).collect());
    for k in ["prod", "prod|rentalpay", "prod|rentalpay|public", "prod|rentalpay|public|g"] {
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

/// 화면들: 콘솔 · 결과, 차트, 운영 작업 기록, 테이블 디자이너, 테스트 데이터 · 관련 행 복사 확인, Redis
fn screens(h: &mut Harness, scenes: &mut Vec<Value>) {
    use qurry_core::ddl::{ColumnDef, ForeignKeyDef, IndexDef, TableDef};
    // 콘솔 + 결과
    let id = h.console_with("select card_brand, count(*) as payments, sum(amount) as total\nfrom payment\ngroup by 1\norder by 3 desc;", 3, 16);
    let res = qurry_core::db::QueryResult {
        columns: [("card_brand", "TEXT"), ("payments", "INT8"), ("total", "NUMERIC")].iter().map(|(n, t)| qurry_core::db::ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
        rows: [("VISA", 1240, "182400000"), ("MASTER", 860, "120900000"), ("AMEX", 210, "61500000"), ("JCB", 95, "9800000")].iter().map(|(b, n, t)| vec![json!(b), json!(n), json!(t)]).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 18,
        has_result_set: true,
        in_transaction: false,
    };
    if let Some(crate::app::TabKind::Console(c)) = h.app.tabs.iter_mut().find(|t| t.id == id).map(|t| &mut t.kind) {
        c.result_title = "Result 1: select card_brand, count(*) as payments,".into();
        c.grid = Some(crate::grid::Grid::new(res));
        c.show_output = false;
    }
    h.app.focus = crate::app::Focus::Editor;
    h.app.status = "4 row(s) retrieved in 18 ms".into();
    scenes.push(app_scene(h, "Console & result", "콘솔 (위) · 결과 표 (아래) · F5 실행", 120, 26));
    // 차트
    h.app.chart_result();
    scenes.push(app_scene(h, "Chart", "⌃k › Chart Result: 첫 글 컬럼별 수 컬럼의 합 (가로 막대)", 120, 26));
    h.app.popup = None;
    // 운영 작업 기록
    let entry = |id: &str, ago: i64, sql: &str, rows: Option<u64>, undo: bool, in_tx: bool| qurry_core::audit::AuditEntry {
        id: id.into(),
        ts: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64 - ago * 60_000,
        conn_id: "prod".into(),
        conn_name: "[PROD] rentalpay".into(),
        database: None,
        sql: sql.into(),
        rows,
        error: None,
        in_tx,
        undo: undo.then(|| "UPDATE …".into()),
        note: None,
    };
    for e in [
        entry("1", 90, "delete from payment where created_at < now() - interval '1 year'", Some(12408), false, false),
        entry("2", 12, "update payment set amount = 0 where id = 4", Some(1), true, true),
        entry("3", 10, "COMMIT", None, false, false),
    ] {
        h.app.state.audit.add(e).unwrap();
    }
    h.app.production_log();
    scenes.push(app_scene(h, "Production log", "⌃k › Production Log: 운영에서 실행한 변경 (↶ undo 가 있으면 ↩ 로 새 콘솔에)", 120, 22));
    h.app.popup = None;
    // 테이블 디자이너 (고치기)
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
    // 테스트 데이터 · 관련 행 복사 확인
    h.app.popup = Some(Popup::Confirm {
        text: "Insert 1,000 generated rows into public.payment?\n\n  id          left to the database (auto / generated)\n  user_id     picked from public.users (id)\n  amount      random values\n  card_brand  one of: VISA, MASTER, AMEX\n  created_at  random values\n\n⚠ Production data source \"[PROD] rentalpay\"\n\nAll rows go in one transaction — if one fails, nothing is inserted.".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(h, "Generate test data", "⌃k › Generate Test Data: 컬럼마다 채우는 법을 보여 주고 확인", 120, 24));
    h.app.popup = Some(Popup::Confirm {
        text: "Copy 29 rows into \"[DEV] rentalpay\"? (in insert order)\n\n  users         1  start · masked: email, name, phone\n  contract      3  child of users\n  payment      25  child of contract\n\nOne transaction; rows already there are skipped. The tables must exist in the target.".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(h, "Copy related rows", "⌃k › Copy Related Rows To: FK 로 이어진 행 · 가린 개인정보 컬럼을 보여 주고 확인", 120, 22));
    h.app.popup = None;
    // Redis
    h.redis_tab(&["session:9f2c", "session:a771", "user:41", "user:41:cart", "queue:mail", "rank:daily"]);
    scenes.push(app_scene(h, "Redis", "Redis 키 브라우저: 키 · 타입 · TTL", 120, 22));
    h.app.focus = crate::app::Focus::Tree;
}

#[tokio::test]
#[ignore = "Storybook 용 TUI 화면을 다시 만들 때만 (pnpm tui-snapshots)"]
async fn kit_snapshots() {
    let mut scenes = vec![atoms(), molecules(), organisms()];
    let mut h = Harness::new().await;
    explorer(&mut h).await;
    scenes.push(app_scene(&mut h, "Explorer & table", "탐색기 (운영 색 · 왼쪽 끝까지 이어지는 줄 색 · 한글 설명), 표 탭, 상태 줄의 PROD 배지", 120, 22));
    h.app.popup = Some(Popup::Confirm {
        text: "Production data source \"[PROD] rentalpay\" — this changes data:\n\n  update payment set amount = 0 where id = 4\n     → 1 row\n  delete from payment where created_at < now() - interval '1 year'\n     → 12,408 rows\n\nRun it?".into(),
        on_yes: Box::new(|_| {}),
    });
    scenes.push(app_scene(&mut h, "Production confirm", "운영 Data Source 에서 바꾸는 문장: 바뀔 행 수를 보여 주고 묻는다", 120, 22));
    h.app.popup = None;
    h.app.palette_actions();
    h.type_str("work");
    scenes.push(app_scene(&mut h, "Actions palette", "⌃k: 모든 명령과 단축키 (입력으로 찾기)", 120, 22));
    h.app.popup = None;
    h.app.conn_form(None);
    scenes.push(app_scene(&mut h, "Data source form", "n (탐색기): 새 Data Source. Workspace · Color 칸은 ◂ ▸ 로 고른다", 120, 34));
    h.app.popup = None;
    h.app.popup = Some(Popup::Help { sel: 0, scroll: 0, record: None, message: None });
    scenes.push(app_scene(&mut h, "Keymap (F1)", "F1: 처음 쓸 키 → 바꿀 수 있는 명령 → 고정 키", 120, 30));
    h.app.popup = None;
    screens(&mut h, &mut scenes);
    h.key("Alt+n");
    h.key("Enter");
    h.key("Alt+1");
    scenes.push(app_scene(&mut h, "Workspaces", "워크스페이스가 둘 이상이면 위쪽 띠 (⌥1…9 · 눌러서 바꾸기)", 120, 22));
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ui/stories/tui-snapshots.json");
    std::fs::write(&out, serde_json::to_string(&json!({ "scenes": scenes })).unwrap()).unwrap();
    println!("wrote {}", out.display());
}
