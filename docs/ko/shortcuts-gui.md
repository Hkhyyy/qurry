# GUI 단축키

[English](../shortcuts-gui.md) | 한국어

앱 안에서도 같은 목록을 **Help › Keymap Reference**, 도구 막대의 ⌨ 버튼, `⇧⌘/` 로 찾아볼 수 있습니다. **Find Action** (`⇧⌘A`) 은 모든 명령을 단축키와 함께 보여 줍니다.
Windows/Linux 에서는 `⌘` 를 `Ctrl`, `⌥` 를 `Alt` 로 읽으세요.

명령 단축키는 **Settings › Keymap** (또는 Keymap Reference 의 **Edit Keymap…**) 에서 바꿀 수 있습니다: *Change* 를 누르고 새 키를 누르세요. 메뉴 · 우클릭 메뉴 · 도구 막대 · Find Action 에서 실행할 수 있는 동작은 모두 여기 있습니다 — 기본 키가 없는 것도 (아래 마지막 표) 있어서 무엇이든 키를 줄 수 있습니다. 키는 그 구역이 말하는 곳에서 동작합니다: 어디서나 (Global), Database Explorer, 쿼리 콘솔, 데이터 그리드. Database Explorer 와 데이터 그리드에서는 Delete · Insert · Backspace 도 키가 될 수 있습니다. 아래 표는 기본값입니다. 설명은 앱 화면에 나오는 문구 그대로 영어로 둡니다.

### 바꿀 수 없는 키

- `⇧⇧` (Search Everywhere) 는 두 번 누르기이고, Switcher (`⌃⇥`) 는 `⌃` 를 뗄 때 열립니다: 둘 다 키 조합이 아닙니다.
- `Esc` 는 언제나 닫기입니다 (팝업, 대화상자, 찾기 막대). 그래서 `⇧⎋` (도구 창 숨기기) 도 그대로 둡니다.
- 이동과 열기: 트리 · 그리드 · 목록 · 팝업의 화살표, Home / End, Page Up / Down, Tab, `↩`. DataGrip 처럼 명령이 아닙니다.
- 입력: 트리에서 글자는 빠른 검색, 그리드에서는 셀 편집을 시작하므로 (`↩` / `F2` 도) 그곳에서는 글자를 키로 쓸 수 없습니다.
- 편집기와 시스템의 키: 그리드의 `⌘c` / `⌘v` / `⌘a`, 편집기의 `⌘/`, `⌘f` (와 찾기 막대의 토글), `⌃Space`.
- 대화상자 안의 키는 그 대화상자에서 동작합니다: 쿼리 기록, Redis 브라우저와 터미널, Data Source 양식. 마우스 동작 (`⌥` 클릭, 끌기) 도 그대로입니다.
- 데이터로 만든 메뉴 항목은 명령이 아니라 선택지입니다: 이름으로 고르는 데이터 소스 · 워크스페이스 · 콘솔, 열의 필터 값, Data Extractor, 자동 새로 고침 간격, Query Execution Settings 의 선택지.

### Database Explorer 에서 여러 노드

`⌘` 클릭은 노드를 선택에 더하거나 빼고, `⇧` 클릭은 마지막으로 누른 노드부터 보이는 노드를 고릅니다. 그냥 클릭하거나 화살표 키를 누르면 다시 한 노드만 고릅니다. 그러면 Go to DDL 과 Edit Data (하나마다 탭, `↩` 도 같음), Copy Name 과 Copy Reference (한 줄에 하나, 참조는 `schema.name`), Drop… 과 Refresh 가 우클릭 메뉴나 그 키로 고른 노드 모두에 실행됩니다. Drop… 은 모든 문장을 보여 주며 한 번만 묻고 (운영 Data Source 는 운영 확인 창), 한 데이터베이스의 객체여야 합니다. 뷰를 먼저, 그다음 테이블을 한 문장으로 지우므로 서로 기대는 객체도 함께 지워집니다. DDL 을 되돌릴 수 없는 MySQL 에서는 문장을 하나씩 실행하고, 실패하면 이미 적용된 문장 수를 알려 줍니다. 한 노드에만 되는 명령은 흐리게 보이고, 툴팁이 한 노드로 돌아가는 법을 알려 줍니다. 고른 노드가 화면에서 사라지면 (접기, 필터, 삭제) 선택에서도 빠집니다.

<!-- keymap:gui:start -->

## Search & Navigation

| macOS | Action |
|---|---|
| `⇧⇧` | Search Everywhere (All / Database / Files / Symbols / Actions, ⇥ switches tabs). Data sources you are not connected to are searched from the list saved the last time they were loaded (marked cached); opening a result connects |
| `⌘o` | Go to Database Object |
| `⌥⌘o` | Go to Symbol (columns) |
| `⇧⌘O` | Go to File (open and recent tabs) |
| `⇧⌘A` | Find Action |
| `⌥⇧⌘F` | Full-text Search in database |
| `⌘e` | Recent Files |
| `⌃⇥ / ⌃⇧⇥` | Switcher (hold ⌃, release to open) |
| `⇧⌘/` | Keymap Reference (this window) |

## In search results (Search Everywhere / Go to)

| macOS | Action |
|---|---|
| `↩` | Select in Database Explorer |
| `⌘↓ / F4` | Edit data (table) / open console (data source; Redis: key browser) |
| `⌘b` | Go to DDL |
| `F1` | Quick documentation |
| `⌥F1` | Select in Database Explorer (the open table; in search results the selected object) |

## Tool windows & appearance

| macOS | Action |
|---|---|
| `⌘1` | Workspace 1 |
| `⌘2` | Workspace 2 |
| `⌘3` | Workspace 3 |
| `⌘4` | Workspace 4 |
| `⌘5` | Workspace 5 |
| `⌘6` | Workspace 6 |
| `⌘7` | Workspace 7 |
| `⌘8` | Workspace 8 |
| `⌘9` | Workspace 9 |
| `⌘0` | Database Explorer (focus / hide) |
| `⇧⌘F` | Find tool window (Full-text Search results) |
| `⇧⎋` | Hide active tool window (in a grid: its Value Editor panel) |
| `⇧⌘F12` | Maximize editor (also: double-click a tab) |
| `⌘,` | Settings (theme, fonts) |
| `` ⌃` `` | Quick switch theme (Dark / Light / High contrast / Sync with OS) |
| `⌃⇧.` | Editor font size bigger |
| `⌃⇧,` | Editor font size smaller |

## Editor tabs

| macOS | Action |
|---|---|
| `⌘w` | Close tab (middle-click also closes); from a grid with the Value Editor open, closes the panel first |
| `⌥ + click ×` | Close other tabs |
| `⇧⌘T` | Reopen closed tab |
| `⇧⌘]` | Next tab |
| `⇧⌘[` | Previous tab |
| `⇧F6` | Rename console (same as Rename File… in the tab menu) |
| `⌥⇥ / ⌥⇧⇥` | Go to the next split (⇧ previous) |
| `right-click tab` | Pin, Split Right/Down (up to 4 panes), Move to Next Group, Close others… |
| `drag tab` | Reorder, or drop on a pane's right/bottom edge to split it (up to 4) |

## Database Explorer

| macOS | Action |
|---|---|
| `⌘↓ / F4` | Edit data (table) / open console (data source; Redis: key browser) |
| `⇧⌘L` | New query console |
| `⇧⌘F10` | Jump to query console |
| `⌘b` | Go to DDL |
| `⌥⌘g` | SQL Generator |
| `⌥⇧⌘G` | Copy DDL to clipboard |
| `⌥⇧⌘B` | Open DDL in console |
| `⌘c` | Copy name (several selected nodes: one per line) |
| `⌥⇧⌘C` | Copy reference |
| `F1` | Quick documentation |
| `⌘F6` | Modify table (columns, keys, indexes, foreign keys) |
| `⇧F6` | Rename table / view / sequence / schema / database |
| `⌘;` | Data source properties |
| `⌘n` | New data source |
| `⌘d` | Duplicate data source |
| `⌘r / ⌥⇧⌘R` | Refresh |
| `⌥⌘y` | Refresh (anywhere) |
| `⌘F2` | Disconnect |
| `⌦ / ⌘⌫` | Remove data source / drop object (asks first) |
| `type letters` | Speed search in the tree |
| `⌘-click` | Add a node to the selection or remove it (Go to DDL, Edit Data, copy, Drop…, Refresh run for every selected node) |
| `⇧-click` | Select the visible nodes from the last clicked one; a plain click or an arrow key selects one node again |

## Query console

| macOS | Action |
|---|---|
| `⌘↩` | Execute: asks what to run when there are several statements (⚙ Query Execution Settings), or the selection |
| `⌘F2` | Cancel running statement |
| `⌥⌘e` | Query history |
| `⌃Space` | Code completion |
| `⌥⌘l` | Reformat code |
| `⌘/` | Line comment |
| `⌥⌘/` | Block comment |
| `⌘d` | Duplicate line |
| `⌘⌫` | Delete line |
| `⌥⇧↑` | Move line up |
| `⌥⇧↓` | Move line down |
| `⌥↑` | Extend selection |
| `⌥↓` | Shrink selection |
| `⌃g` | Select next occurrence |
| `⌃⌘g` | Select all occurrences |
| `⇧⌘U` | Toggle case |
| `⌘l` | Go to line |
| `⌘f` | Find in editor (⌥⌘r regex · ⌥⌘c match case · ⌥⌘w words) |
| `⌘r` | Replace in editor ($1 groups with regex) |

## Data grid (results & table editor)

| macOS | Action |
|---|---|
| `↩ / F2` | Edit cell |
| `⇧↩` | Open in Value Editor |
| `⌘↩ · ⎋` | In the Value Editor panel: apply the value · back to the grid (it follows the current cell) |
| `⌥⌘n` | Set NULL |
| `⌥⌘d` | Set DEFAULT |
| `⌘n` | Add Row |
| `⌘d` | Clone Row |
| `⌘⌫` | Delete Row |
| `⌥⌘z` | Revert Selected |
| `⌘↩` | Submit Changes |
| `⌘c / ⌘v` | Copy (with chosen extractor; CSV / TSV copy values only — right-click › Copy with Headers adds the column names) / paste |
| `⌘a` | Select all |
| `⌘f` | Find on Current Page |
| `⌘r` | Reload Page |
| `⌘F2` | Cancel Running Statements |
| `⌘b` | Go to DDL |
| `click header · ⌥ click` | Sort · add to multi-column sort |

## Query history & Redis browser

| macOS | Action |
|---|---|
| `⌦ / ⌘⌫` | Delete the selected history entry |
| `⌘r` | Redis: scan keys again |
| `⌘s` | Redis: save the value being edited |
| `⌃l` | Redis terminal: clear |

## Other actions (menus, toolbars, Find Action — give them a key in Settings › Keymap)

| macOS | Action |
|---|---|
| `—` | Edit Keymap… |
| `—` | Export Data Sources… |
| `—` | Import Data Sources… |
| `—` | Theme: Dark |
| `—` | Theme: Light |
| `—` | Theme: High Contrast |
| `—` | Theme: Sync with OS |
| `—` | Reset Editor Font Size |
| `—` | Force Refresh |
| `—` | Collapse All |
| `—` | Show System Schemas |
| `—` | New Materialized View… |
| `—` | Generate SELECT Statement |
| `—` | Generate INSERT Statement |
| `—` | Generate UPDATE Statement |
| `—` | Generate DELETE Statement |
| `—` | Truncate… |
| `—` | Export Table Data… |
| `—` | Import Data into New Table… |
| `—` | Sessions & Locks |
| `—` | Slow Queries |
| `—` | Table Sizes & Indexes |
| `—` | Server Overview |
| `—` | Users & Privileges |
| `—` | Compare & Migrate… |
| `—` | Close Other Tabs |
| `—` | Close All but Pinned |
| `—` | Close Tabs to the Left |
| `—` | Close Tabs to the Right |
| `—` | Close All Tabs |
| `—` | Pin / Unpin Tab |
| `—` | Copy Tab Reference |
| `—` | Copy Data Source Name |
| `—` | Split and Move Right |
| `—` | Split and Move Down |
| `—` | Move Tab to Next Group |
| `—` | Open Tab in Next Group |
| `—` | Move Tab to New Workspace… |
| `—` | Duplicate Console |
| `—` | DDL: Show SQL |
| `—` | DDL: Show Structure |
| `—` | DDL: Reload |
| `—` | DDL: Copy to Clipboard |
| `—` | DDL: Save to File |
| `—` | DDL: Run in Console |
| `—` | New Workspace… |
| `—` | Rename Workspace… |
| `—` | Split Right |
| `—` | Split Down |
| `—` | Explain Plan |
| `—` | Transaction Mode: Manual |
| `—` | Transaction Mode: Auto |
| `—` | Pin / Unpin Result Tab |
| `—` | Close Result Tab |
| `—` | Close All Unpinned Result Tabs |
| `—` | Clear Output |
| `—` | Generate UUID |
| `—` | Copy with Headers |
| `—` | Preview Pending Changes |
| `—` | View as Table |
| `—` | View as Transposed Table |
| `—` | View as Chart |
| `—` | Show Filter |
| `—` | Sort via ORDER BY |
| `—` | Reset View |
| `—` | View Query |
| `—` | Copy Query to Console |
| `—` | Copy Query to Clipboard |
| `—` | Commit |
| `—` | Roll Back |
| `—` | Explain Analyse |
| `—` | Export Data… |
| `—` | Copy as IN List |
| `—` | AI Search (RAG)… |
| `—` | AI Search (RAG) — Index This Table… |
| `—` | Delete Workspace… |
| `—` | Remove Data Source from This Workspace |
| `—` | Production Log… |
| `—` | Snippets… |
| `—` | Save Query as Snippet… |
| `—` | Show Diagram (ERD) |
| `—` | Back Up Database… |
| `—` | Restore Database… |
| `—` | Import Data from File… |
| `—` | Generate Test Data… |
| `—` | New Table… |
| `—` | New View… |
| `—` | New Schema… |
| `—` | New Database… |
| `—` | New Sequence… |
| `—` | Copy Related Rows To… |
| `—` | Manage Shown Schemas… |
| `—` | Unsplit All |

<!-- keymap:gui:end -->

---

[← 사용설명서](README.md)
