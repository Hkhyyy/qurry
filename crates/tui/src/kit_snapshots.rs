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
    h.key("Alt+n");
    h.key("Enter");
    h.key("Alt+1");
    scenes.push(app_scene(&mut h, "Workspaces", "워크스페이스가 둘 이상이면 위쪽 띠 (⌥1…9 · 눌러서 바꾸기)", 120, 22));
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ui/stories/tui-snapshots.json");
    std::fs::write(&out, serde_json::to_string(&json!({ "scenes": scenes })).unwrap()).unwrap();
    println!("wrote {}", out.display());
}
