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

/// 공통 조각 (kit.rs) 을 한 화면에
fn components() -> Value {
    scene("Components", "kit.rs 의 공통 조각: 패널(포커스 · 아님), 팝업, 고른 줄, 배지, 안내 줄, 색 토큰", 132, 24, |f| {
        let [top, mid, bot] = Layout::vertical([Constraint::Length(7), Constraint::Length(9), Constraint::Min(6)]).areas(f.area());
        let [a, b] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(top);
        f.render_widget(Paragraph::new(vec![Line::raw("panel(title, true)"), Line::styled("포커스: 굵은 강조 테두리", Style::default().fg(DIM))]).block(panel(" Focused ", true)), a);
        f.render_widget(Paragraph::new(vec![Line::raw("panel(title, false)"), Line::styled("흐린 얇은 테두리", Style::default().fg(DIM))]).block(panel(" Not focused ", false)), b);
        let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(mid);
        let rows = vec![
            Line::raw(" ▦ users").style(Style::default().bg(selected(true))),
            Line::raw(" ▦ orders"),
            Line::raw(" ▦ payments").style(Style::default().bg(selected(false))),
            Line::styled(" selected(true) · 보통 · selected(false)", Style::default().fg(DIM)),
        ];
        f.render_widget(Paragraph::new(rows).block(panel(" Rows ", false)), l);
        let inner = popup(f, Rect { x: r.x + 1, y: r.y, width: r.width - 2, height: r.height }, " Confirm ");
        let mut keys = key_hint("y / Enter", "yes");
        keys.push(Span::styled(" · ", Style::default().fg(DIM)));
        keys.extend(key_hint("n / Esc", "no"));
        f.render_widget(Paragraph::new(vec![Line::raw("Delete snippet \"user orders\"?"), Line::raw(""), Line::from(keys)]), inner);
        let mut sw = vec![Line::from(vec![badge("PROD", DANGER), Span::raw(" "), badge("READ-ONLY", SEL), Span::raw(" "), badge("v0.2.5 available", WARN)])];
        sw.push(Line::raw(""));
        let toks = [("ACCENT", ACCENT), ("SEL", SEL), ("SURFACE", SURFACE), ("POPUP_BG", POPUP_BG), ("OK", OK), ("WARN", WARN), ("DANGER", DANGER), ("NUMBER", NUMBER), ("C_DB", C_DB), ("C_VIEW", C_VIEW), ("C_ROUTINE", C_ROUTINE)];
        sw.push(Line::from(toks.iter().flat_map(|(n, c)| [Span::styled("  ", Style::default().bg(*c)), Span::styled(format!(" {n}  "), Style::default().fg(DIM))]).collect::<Vec<_>>()));
        f.render_widget(Paragraph::new(sw).block(panel(" Badges · color tokens ", false)), bot);
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
    let mut scenes = vec![components()];
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
