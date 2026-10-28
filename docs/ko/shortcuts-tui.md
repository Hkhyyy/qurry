# TUI 단축키

[English](../shortcuts-tui.md) | 한국어

키는 macOS 에 보이는 대로 씁니다: `⌃` Control, `⌥` Option, `⇧` Shift. Linux 에서는 `⌃` 를 `Ctrl`, `⌥` 를 `Alt` 로 읽으세요 (그곳 TUI 화면도 그렇게 보입니다).

`qurry-tui` 안에서 **F1** 을 누르거나 `qurry-tui --help` 를 실행하면 같은 목록이 나옵니다. F1 창 위쪽의 **Commands** 는 바꿀 수 있습니다: 하나를 고르고 Enter, 그다음 새 키 (`a` 두 번째 키 추가, `Del` 기본값으로). 마우스도 됩니다: 클릭으로 고르기, 더블클릭으로 열기 · 편집, 휠로 스크롤.
⌃Enter 는 kitty 키보드 프로토콜을 지원하는 터미널 (iTerm2, kitty, WezTerm, Ghostty) 에서만 구분됩니다. 다른 곳에서는 F5 나 ⌥Enter 를 쓰세요.

**처음이라면 `⌃p` 하나로 충분합니다.** 테이블 · 컬럼 · 열린 탭 · 명령을 찾습니다 (연결하지 않은 Data Source 는 마지막으로 읽었을 때 저장한 목록에서 찾음). 상태 줄에는 지금 보고 있는 곳 (탐색기, 콘솔, 테이블, 결과, Redis) 의 키와 가장 많이 쓰는 전역 키가 늘 나옵니다. F1 창도 같은 짧은 목록으로 시작합니다. Shift 가 필요 없는 키는 소문자로 (`⌃k`), 대문자는 Shift 를 뜻합니다 (`D`, `⌥⇧X`).

| 키 | 하는 일 |
|---|---|
| `⌃p` | 전부 찾기: 테이블, 컬럼, 탭, 명령 (모르겠으면 여기부터) |
| `⌃k` | 모든 명령과 단축키 |
| `⌃l` | 고른 Data Source 의 새 쿼리 콘솔 |
| `F5 / ⌃Enter / ⌥Enter` | 실행 (문장이 여러 개면 무엇을 실행할지 물음) |
| `⌃e` | 최근 탭 |
| `⌃w` | 탭 닫기 |
| `Enter` (탐색기) | 연결 · 테이블 열기 |
| `s` (탐색기) | 보일 데이터베이스 · 스키마 고르기 |
| `Esc` | 뒤로 (탐색기로, 또는 결과에서 편집기로) |
| `⌃q` | 종료 |

아래 전체 표의 설명은 앱 화면 (F1) 에 나오는 문구 그대로 영어로 둡니다.

<!-- keymap:tui:start -->

## Commands (default keys — change them in the F1 window)

| Key | Action |
|---|---|
| `⌃q` | Quit |
| `F1` | Keymap |
| `⌃k` | Actions Palette |
| `⌃p` | Search Everywhere (objects · columns · tabs · actions) |
| `⌃o` | Go to Database Object |
| `⌃b` | Go to DDL |
| `⌃e` | Recent Tabs / Switcher |
| `⌃l` | New Query Console |
| `⌃w` | Close Tab |
| `⌥Right / ⌃PgDn` | Next Tab |
| `⌥Left / ⌃PgUp` | Previous Tab |
| `⌥0` | Database Explorer |
| `⌥n` | New Workspace… |
| `⌥⇧N` | Rename Workspace… |
| `F6` | Cycle Focus (explorer → editor → results) |
| `⌥\` | Split Right (focused pane, up to 4) |
| `⌥-` | Split Down (focused pane, up to 4) |
| `⌥Tab` | Go to the Next Split |
| `⌥⇧Left` | Move Split Divider Left / Up |
| `⌥⇧Right` | Move Split Divider Right / Down |
| `⌥[` | Database Explorer Narrower |
| `⌥]` | Database Explorer Wider |
| `F5 / ⌃Enter / ⌥Enter` | Execute (asks what to run — see Query Execution Settings) |
| `⌥e` | Explain Plan |
| `⌃Space` | Code Completion |
| `⌃c` | Cancel Running Statement |
| `⌥t` | Toggle Transaction Mode (Auto/Manual) |
| `⌥c` | Commit |
| `⌥r` | Roll Back |
| `⌥o` | Toggle Output / Result |
| `⌃⌥Up` | Go to the Split Above |
| `⌃⌥Down` | Go to the Split Below |
| `⌃⌥Left` | Go to the Split on the Left |
| `⌃⌥Right` | Go to the Split on the Right |
| `⌥⇧Up` | Shrink Editor (console) |
| `⌥⇧Down` | Grow Editor (console) |
| `⌃s` | Submit Changes |
| `⌃f` | Find / Replace in Editor (regex) |
| `⌃F2` | Disconnect |

## Database Explorer

| Key | Action |
|---|---|
| `↑ ↓  (j k)` | Move |
| `→ / ←  (l h)` | Expand / collapse (← on a child jumps to its parent) |
| `Enter / double-click` | Expand / collapse (connects) · open table data · open a Redis key · show DDL of routines |
| `F4 / ⌃↓` | Open a query console for the data source / database (Redis: the key browser) |
| `/` | Filter loaded nodes (/ again edits it · empty or Esc clears) |
| `n / e / D` | New / edit / remove data source |
| `w` | Move the data source to another workspace (⌥1…9 switches · ⌥n new · ⌥⇧N rename) |
| `c / X` | Connect / disconnect |
| `r / F5` | Refresh |
| `s` | Manage shown databases and schemas (Space check · Enter apply · shared with the GUI) |
| `y  ⌘c` | Copy name (OSC 52 clipboard) |
| `PgUp / PgDn · Home / End` | Page · first / last node |
| `?` | Keymap (same as F1) |
| `Tab / Esc` | Go to the open tab |

## Query console

| Key | Action |
|---|---|
| `completion list` | Opens as you type (or ⌃Space) · ↑↓ choose · Enter/Tab insert · Esc close |
| `⇧Tab` | Go to the result grid (Esc / Tab returns) |
| `Esc` | Go to the explorer |
| `⌃u / ⌃r` | Undo / redo (editor) |
| `⌃f` | Find / replace bar: ⌥x regex · ⌥c match case · ⌥w words · Enter next (↑ previous) · Tab to Replace: Enter one, ⌥a all ($1 groups) · Esc close |
| `⌘a  ⌫` | Select all, then delete (or ⌃k › Clear Console) |
| `click / drag  ⌘c` | Move the cursor / select text (runs only the selection) · copy the selection |
| `mouse drag` | Drag the editor/result border to resize |

## Tabs & split (mouse)

| Key | Action |
|---|---|
| `drag a tab` | Onto a pane's right / bottom edge = split it (up to 4 panes) · onto another pane or tab bar = move · within a bar = reorder |
| `drag the border` | Resize the Database Explorer (its right border) or the panes (the border between them; in a top/bottom split also the empty part of the lower tab bar) |
| `click a pane` | Focus it |
| `⇧wheel / trackpad` | Scroll the grid and the explorer sideways (⇧← / ⇧→ in the explorer) |

## Data grid (results & table data)

| Key | Action |
|---|---|
| `arrows  (h j k l)` | Move |
| `PgUp / PgDn · g / G` | Page · first / last row (more rows load automatically) |
| `Home / End` | First / last column |
| `⇧↑↓←→ / drag` | Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or drag the bar on the bottom border |
| `Enter / F2` | Edit cell (table data) |
| `n` | Set NULL (table data) |
| `a / Insert` | Add row (table data) |
| `d / Delete` | Mark row deleted, again = undo (table data) |
| `⌃z` | Revert pending changes (table data) |
| `s` | Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows) |
| `w or / · o` | Edit WHERE · ORDER BY (table data) |
| `r` | Reload (table data) |
| `x` | Export data (CSV, TSV, JSON, SQL INSERT, Markdown → file or clipboard) |
| `y  ⌘c` | Copy cell value (a selected range: as TSV, pastes into spreadsheets as cells) |
| `Y` | Copy as an IN list: ('a', 'b', …) — the selected range, or the whole column |
| `⌘v` | Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data) |
| `Esc` | Back (query results: to the editor) |

## DDL / routine view

| Key | Action |
|---|---|
| `↑ ↓  (j k) · PgUp / PgDn` | Scroll |
| `y  ⌘c` | Copy the text |
| `Esc` | Back to the explorer |

## Database Admin tab (⌃k › Database Admin…)

| Key | Action |
|---|---|
| `r · a` | Refresh · auto-refresh on/off (sessions and locks refresh every 3 s) |
| `c · K` | Cancel the query · terminate the session of the selected row (asks first) |
| `Enter · g` | Open the row's query in a console · generate SQL from the row (DROP INDEX, VACUUM, REVOKE…) |
| `p` | Pick another report |

## Redis browser (Medis style)

| Key | Action |
|---|---|
| `→ / Enter · ←` | Folder: expand · collapse (← on a key: go to its folder) |
| `Enter / → / Tab` | Key: go to the value |
| `/` | Key pattern (SCAN MATCH) |
| `m` | Scan more |
| `r` | Rescan |
| `a` | Add key (SET / HSET / RPUSH / SADD / ZADD …) |
| `D / Delete` | Delete key |
| `R` | Rename key |
| `t` | Set TTL (empty = persist) |
| `:` | Run any command |
| `e / Enter (value)` | Edit selected element / string |
| `a (value)` | Add element |
| `x / Delete (value)` | Delete element |
| `Esc / Tab / ← (value)` | Back to keys |

## Data source form

| Key | Action |
|---|---|
| `↑ ↓ / Tab` | Move between fields |
| `type / Backspace` | Edit text fields |
| `Space / ← →` | Toggle on/off · change choice (Type, Color, SSL mode, Redis mode…) |
| `⌃t` | Test connection |
| `⌃y` | Trust the SSH server's host key (after ⌃t showed its fingerprint) |
| `Enter / ⌃s` | Save |
| `Esc` | Cancel |

## Popups

| Key | Action |
|---|---|
| `⌃b` | Go to DDL (in Search Everywhere / Go to Database Object) |
| `type` | Filter (palettes) |
| `↑ ↓ / PgUp PgDn` | Move / scroll |
| `Enter` | Run / confirm |
| `y / n` | Answer confirmations |
| `Esc` | Close |

<!-- keymap:tui:end -->

---

[← 사용설명서](README.md)
