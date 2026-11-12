# TUI 단축키

[English](../shortcuts-tui.md) | 한국어

키는 macOS 에 보이는 대로 씁니다: `⌃` Control, `⌥` Option, `⇧` Shift. Linux 에서는 `⌃` 를 `Ctrl`, `⌥` 를 `Alt` 로 읽으세요 (그곳 TUI 화면도 그렇게 보입니다).

`qurry-tui` 안에서 **F1** 을 누르거나 `qurry-tui --help` 를 실행하면 같은 목록이 나옵니다. F1 창 위쪽의 **Commands** 는 바꿀 수 있습니다: 하나를 고르고 Enter, 그다음 새 키 (`a` 두 번째 키 추가, `Del` 기본값으로). `⌃k` 팔레트의 모든 명령이 기본 키가 없는 것까지 있고, 이어서 Database Explorer 와 데이터 그리드의 한 글자 명령 (`n` New Data Source, `a` Add Row …) 이 나옵니다. 이것도 같은 방법으로 바꿀 수 있고, 그 창에서만 동작하며 글자 하나여도 됩니다. 마우스도 됩니다: 클릭으로 고르기, 더블클릭으로 열기 · 편집, 휠로 스크롤.
⌃Enter 는 kitty 키보드 프로토콜을 지원하는 터미널 (iTerm2, kitty, WezTerm, Ghostty) 에서만 구분됩니다. 다른 곳에서는 F5 나 ⌥Enter 를 쓰세요.
tmux 에서 `⌃b` (Go to DDL) 는 tmux prefix 입니다: 두 번 누르거나, 그것과 ⌥ 키는 [터미널에서 쓰는 TUI](tui.md#tmux-와-다른-멀티플렉서) 를 보세요.

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

### 바꿀 수 없는 키

명령 다음에 나오는 화면별 키 표는 바꿀 수 없습니다: 이동 · 열기 · 편집 · 복사 · 필터 · 정렬을 할 뿐 명령을 실행하지 않으므로, 탐색기나 그리드 키로도 가져갈 수 없습니다.

- 이동: 화살표, `j k h l`, `g G`, Page Up / Down, Home / End; `Enter` / `F2` 열기 · 편집, `Esc` / `Tab` 돌아가기.
- 탐색기의 `/` 필터, `y` 이름 복사, `F4` 콘솔 열기; 그리드의 `y` 복사, `s` 정렬, `w` / `/` / `o` WHERE · ORDER BY 편집, `r` 다시 읽기.
- `⌥1…9` 워크스페이스 전환 (그 키를 가진 명령이 없을 때), 편집기의 `⌃u` / `⌃r` 실행 취소 · 다시 실행, 찾기 막대의 키.
- 팝업, Data Source 양식, Redis 브라우저, Database Admin 탭 안의 키는 그 화면에서 동작합니다. F1 창의 키도 그렇습니다.

### 탐색기에서 여러 노드

`m` 은 커서의 노드를 표시하거나 표시를 지웁니다 (GUI 의 `⌘` 클릭). 커서를 옮겨도 표시는 남고, `Esc` 로 모두 지웁니다. 그러면 Go to DDL (`⌃b`) 과 `F4` (하나마다 탭), `y` (한 줄에 `schema.name` 하나, GUI 의 Copy Reference), `Delete` (Drop…), 새로 고침이 화면에 보이는 표시한 노드 모두에 동작합니다. 접힌 폴더나 필터로 가려진 표시는 탐색기 제목에 "(N hidden)" 으로 따로 셉니다. Drop… 은 모든 문장을 보여 주며 한 번만 묻고 (운영 Data Source 는 한 객체일 때처럼 빨간 창), 한 데이터베이스의 테이블·뷰·시퀀스여야 합니다. 문장 순서는 GUI 와 같고 (기대는 쪽이 먼저, PostgreSQL 은 종류마다 한 문장을 한 트랜잭션으로, MySQL 은 객체마다 한 문장을 하나씩), 실패하면 적용된 것과 실행하지 않은 것을 알려 줍니다. 실패해도 탐색기를 다시 읽고 표시를 지웁니다. 표시한 노드 중 맞지 않는 것이 있으면 상태 줄이 그렇다고, 표시를 지우는 법과 함께 알려 줍니다.

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
| `⌃F6` | Modify Table… (change columns, keys, indexes, foreign keys → ALTER) |
| `⇧F6` | Rename… (table, view, sequence, schema, database) |
| `⌥\` | Split Right (focused pane, up to 4) |
| `⌥-` | Split Down (focused pane, up to 4) |
| `⌥Tab` | Go to the Next Split |
| `⌥⇧Left` | Move Split Divider Left / Up |
| `⌥⇧Right` | Move Split Divider Right / Down |
| `⌥[` | Database Explorer Narrower |
| `⌥]` | Database Explorer Wider |
| `F5 / ⌃Enter` | Execute (asks what to run — see Query Execution Settings) |
| `⌥Enter` | Quick Fixes (an underlined unknown name's closest names; elsewhere Execute) |
| `⌥e` | Explain Plan |
| `⌃Space` | Code Completion |
| `⌥⇧F` | Reformat Code (the selection or the statement at the cursor) |
| `⌃c` | Cancel Running Statement |
| `⌥t` | Toggle Transaction Mode (Auto/Manual) |
| `⌥c` | Commit |
| `⌥r` | Roll Back |
| `⌥o` | Toggle Output / Result |
| `⌥.` | Next Result Tab |
| `⌥,` | Previous Result Tab |
| `⌃⌥Up` | Go to the Split Above |
| `⌃⌥Down` | Go to the Split Below |
| `⌃⌥Left` | Go to the Split on the Left |
| `⌃⌥Right` | Go to the Split on the Right |
| `⌥⇧Up` | Shrink Editor (console) |
| `⌥⇧Down` | Grow Editor (console) |
| `⌃s` | Submit Changes |
| `⌃f` | Find / Replace in Editor (regex) |
| `⌃F2` | Disconnect |

## Commands without a default key (give them one in the F1 window)

| Key | Action |
|---|---|
| — | Show Log |
| — | Import Data Sources… (DataGrip, IntelliJ, DBeaver, a Qurry export) |
| — | Data Source Properties… |
| — | New Data Source… |
| — | Refresh |
| — | Query History… (shared with the GUI) |
| — | Pin / Unpin Result Tab |
| — | Close Result Tab |
| — | Value Panel (the current cell's whole value, beside the grid) |
| — | Narrow Column (the current one) |
| — | Widen Column (the current one) |
| — | Fit Column to its content (the rows loaded so far; again: back to the width before) |
| — | Fit All Columns |
| — | Reset Column Widths (back to the computed widths) |
| — | Explain Analyze (runs the statement: real times and row counts) |
| — | Query Execution Settings (what ⌃Enter runs) |
| — | Clear Console (delete all text) |
| — | Edit Data (open table) |
| — | Revert Changes |
| — | Add Row |
| — | Delete Row |
| — | Set NULL |
| — | Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown) |
| — | Copy Column as IN List  ('a', 'b', …) |
| — | Mark Row (add the row to the selection or take it out: y copies, d deletes the marked rows) |
| — | Find Similar Rows (pgvector, by the vector column) |
| — | AI Search (RAG)… — ask a table in plain words |
| — | Build RAG Index… (embed the rows) |
| — | New RAG Index from the Selected Table… |
| — | Set OpenAI API Key (RAG)… |
| — | Switch Workspace… |
| — | Delete Workspace… |
| — | Move Tab to Workspace… |
| — | Add Data Source to This Workspace… |
| — | Move Data Source to Another Workspace… |
| — | Remove Data Source from This Workspace |
| — | Production Log… (changes run on Production data sources, undo SQL) |
| — | Snippets… |
| — | Save Query as Snippet… |
| — | Show Diagram (ERD): foreign keys of a schema, as text + Mermaid |
| — | Compare Schemas… (DDL to make one schema look like another) |
| — | Compare Table Data… (sync SQL by primary key) |
| — | Run Migrations… (folder of V1__name.sql files) |
| — | Back Up Database… (pg_dump / mysqldump) |
| — | Restore Database… (from a .dump / .sql backup) |
| — | Database Admin… (sessions & locks, slow queries, sizes, indexes, vacuum, server, privileges) |
| — | Import Data from File… (CSV, TSV, JSON, Excel) |
| — | Generate Test Data… (fill the selected table with realistic rows) |
| — | New Table… (columns, keys, indexes, foreign keys — SQL shown before it runs) |
| — | New View… |
| — | New Schema… (PostgreSQL) |
| — | New Database… |
| — | New Sequence… (PostgreSQL) |
| — | Chart Result (bars: sum of a number column by the first text column) |
| — | Copy Related Rows To… (rows + foreign-key related rows into another data source, personal data masked) |
| — | Cancel Export/Import |
| — | Remove Data Source |
| — | Connect |
| — | Mark / Unmark Node (Go to DDL, open, copy, drop and refresh act on every marked node; Esc clears) |
| — | Drop… (the selected object, or the marked objects of one database: one confirmation) |
| — | Manage Shown Schemas… (databases · schemas in the explorer) |
| — | Unsplit All |
| — | Trust SSH Host Key (after checking its fingerprint) |
| — | Password Key… (keep it in the config folder or the system keychain) |
| — | Keyword Case… (UPPER or lower SQL keywords as you type and in Reformat Code; shared with the GUI) |
| — | Result Tabs… |
| — | Grid Alignment… (where cells sit per kind of value: NULL, numbers, text …; shared with the GUI) |
| — | Status Bar Items… (what the status line shows for a grid: position, shown columns, Sum, Size …; shared with the GUI) |
| — | Theme… (the GUI's built-in and custom themes, or Sync with OS by the terminal background) |
| — | Check for Updates |
| — | Update Channel… (stable releases only, or beta: also every merge to main) |
| — | Update Qurry & Restart (download the latest release) |
| — | Open GitHub |

## Database Explorer keys (when the explorer has focus; change them in the F1 window)

| Key | Action |
|---|---|
| `?` | Keymap |
| `e` | Data Source Properties… |
| `n` | New Data Source… |
| `r` | Refresh |
| `X` | Disconnect |
| `w` | Move Data Source to Another Workspace… |
| `D` | Remove Data Source |
| `c` | Connect |
| `m` | Mark / Unmark Node (Go to DDL, open, copy, drop and refresh act on every marked node; Esc clears) |
| `Delete` | Drop… (the selected object, or the marked objects of one database: one confirmation) |
| `s` | Manage Shown Schemas… (databases · schemas in the explorer) |

## Data grid keys (when a grid has focus; change them in the F1 window)

| Key | Action |
|---|---|
| `v` | Value Panel (the current cell's whole value, beside the grid) |
| `<` | Narrow Column (the current one) |
| `>` | Widen Column (the current one) |
| `=` | Fit Column to its content (the rows loaded so far; again: back to the width before) |
| `⌃z` | Revert Changes |
| `a / Insert` | Add Row |
| `d / Delete` | Delete Row |
| `n` | Set NULL |
| `x` | Export Data… (CSV, TSV, JSON, SQL INSERT, Markdown) |
| `Y` | Copy Column as IN List  ('a', 'b', …) |
| `m` | Mark Row (add the row to the selection or take it out: y copies, d deletes the marked rows) |

## Database Explorer

| Key | Action |
|---|---|
| `↑ ↓  (j k)` | Move |
| `→ / ←  (l h)` | Expand / collapse (← on a child jumps to its parent) |
| `Enter / double-click` | Expand / collapse (connects) · open table data · show DDL of routines · Redis database: the source's Redis tab on it |
| `click (Redis database)` | Switch the source's open Redis tab to that database |
| `F4 / ⌃↓` | Open a query console for the data source / database (Redis: the source's Redis tab) |
| `/` | Filter loaded nodes (/ again edits it · empty or Esc clears) |
| `F5` | Refresh (the run key refreshes in the explorer) |
| `y  ⌘c` | Copy reference: schema.name, as the GUI's Copy Reference (OSC 52 clipboard; marked nodes: one per line) |
| `PgUp / PgDn · Home / End` | Page · first / last node |
| `Tab / Esc` | Go to the open tab (Esc clears the marks first) |

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
| `⇧↑↓←→ / drag` | Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or click / drag the bar on the bottom border (its heavier part is the columns shown; ◀ cols 3–8 of 14 ▶ at its right end) |
| `Enter / F2` | Edit cell (table data) |
| `s` | Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows) |
| `w or / · o` | Edit WHERE · ORDER BY (table data) |
| `r` | Reload (table data) |
| `y  ⌘c` | Copy cell value (a selected range or marked rows: as TSV, pastes into spreadsheets as cells) |
| `⌘v` | Paste into cells from the selected one (TSV or CSV fills several, an empty field is NULL; extra lines add rows; table data) |
| `Esc` | Back (query results: to the editor) |

## DDL / routine view

| Key | Action |
|---|---|
| `↑ ↓  (j k) · PgUp / PgDn` | Scroll |
| `v` | Table DDL: switch between SQL and Structure (columns, keys, indexes, foreign keys, triggers, options); kept for new DDL tabs |
| `r` | Reload what the tab shows (the DDL, or the Structure view) |
| `y  ⌘c` | Copy the text (a table's DDL: the SQL, in either view) |
| `Esc` | Back to the explorer |

## Database Admin tab (⌃k › Database Admin…)

| Key | Action |
|---|---|
| `r · a` | Refresh · auto-refresh on/off (sessions and locks refresh every 3 s) |
| `c · K` | Cancel the query · terminate the session of the selected row (asks first) |
| `Enter · g` | Open the row's query in a console · generate SQL from the row (DROP INDEX, VACUUM, REVOKE…) |
| `p` | Pick another report |
| `d` | Run the report on another database (MySQL: schema); the grid's title names it |

## Redis browser (Medis style)

| Key | Action |
|---|---|
| `→ / Enter · ←` | Folder: expand · collapse (← on a key: go to its folder) |
| `Enter / → / Tab` | Key: go to the value |
| `d` | Show another database (with key counts; one Redis tab per data source) |
| `/` | Filter keys (SCAN MATCH pattern, e.g. prod:user:*) |
| `m` | Scan more |
| `r` | Rescan |
| `a` | Add key (SET / HSET / RPUSH / SADD / ZADD …) |
| `D / Delete` | Delete key |
| `R` | Rename key |
| `t` | Set TTL (empty = persist) |
| `:` | Command line: Tab completes commands (with arguments) and key names, ↑↓ history, HELP command; read-only runs reads only, Production asks before writes, FLUSHDB / DEL always ask |
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
| `Go to DDL key` | DDL of the selected object (in Search Everywhere / Go to Database Object) |
| `type` | Filter (palettes) |
| `↑ ↓ / PgUp PgDn` | Move / scroll |
| `Enter` | Run / confirm |
| `y / n` | Answer confirmations |
| `Esc` | Close |

<!-- keymap:tui:end -->

---

[← 사용설명서](README.md)
