# UI 규격

화면은 이 폴더 (GUI) 와 `crates/tui/src/kit` (TUI) 의 조각으로만 만든다.
화면 코드에 맨 HTML (`<button>`, `<input>`, `<span className="muted">` …), 숫자 px · 숫자 색을 직접 쓰지 않는다.
모양을 바꿀 일이 있으면 조각을 고치고, 화면 코드는 건드리지 않는다.

미리 보기: `pnpm storybook` → http://localhost:6006 — GUI 는 단계별 (1 Atoms · 2 Molecules · 3 Organisms · 4 Grid), 위쪽에서 테마 바꾸기. TUI 는 `pnpm tui-snapshots` 가 실제 TUI 코드로 그린 화면.

조각은 세 단계로 나눈다 (아토믹 디자인).

| 단계 | 뜻 | 예 |
|---|---|---|
| 원자 (atoms) | 더 쪼갤 수 없는 것 | 글자 · 코드 · 단축키 · 버튼 · 입력칸 · 배지 · 구분선 |
| 분자 (molecules) | 원자 몇 개를 묶은 한 줄 · 한 칸 | 목록 한 줄 · 트리 한 줄 · 상세 줄 · 메뉴 항목 · 폼 칸 · 탭 |
| 유기체 (organisms) | 분자를 모은 덩어리 | 도구 막대 · 대화상자 · 확인 창 · 메뉴 · 팝오버 · 목록 · 표 |

## 토큰 (`tokens.css`)

| 종류 | 토큰 | 값 (dark) | 쓰는 곳 |
|---|---|---|---|
| 바탕 | `--ui-bg` | #1e1f22 | 편집기, 표, 입력칸 |
| | `--ui-surface` | #2b2d30 | 패널, 도구 막대, 대화상자, 메뉴 |
| | `--ui-surface-2` | #26282b | 한 단계 낮은 면 |
| | `--ui-hover` · `--ui-selected` | #33353a · #2e436e | 올림 · 고른 줄 |
| 선 | `--ui-border` · `--ui-border-strong` | #393b40 · #4e5157 | 구역 · 입력칸 테두리 |
| 글자 | `--ui-text` · `--ui-text-muted` · `--ui-text-on-accent` | #dfe1e5 · #868a91 · #fff | 본문 · 설명 · 강조색 위 |
| 색 | `--ui-accent` · `--ui-danger` · `--ui-warn` · `--ui-ok` | #3574f0 · #f75464 · #e5a64b · #5fb865 | 주 버튼 · 위험 · 주의 · 성공 |
| 간격 | `--ui-sp-1` … `--ui-sp-6` | 2 · 4 · 8 · 12 · 16 · 24px | padding · gap · margin 은 이 값만 |
| 둥글기 | `--ui-radius-sm` / `-md` / `-lg` | 4 · 6 · 10px | 버튼 / 메뉴 · 목록 / 대화상자 |
| 글자 크기 | `--ui-fs-xs` / `-sm` / `-md` / `-lg` | 11 · 12 · 13 · 14px | 배지 / 보조 / 본문 / 제목 |
| 높이 | `--ui-h-sm` / `-md` / `-lg` | 22 · 26 · 30px | 작은 버튼 / 버튼 · 입력칸 / 도구 막대 |
| 그림자 · 쌓기 | `--ui-shadow-pop` / `-modal` · `--ui-z-modal` 100 · `-menu` 200 · `-popover` 300 · `-dialog` 400 | | 확인 창이 늘 맨 위 |

라이트 · 고대비 테마는 같은 이름에 다른 값만 준다.

## GUI 원자 (`atoms.tsx`)

| 조각 | 규칙 |
|---|---|
| `Text` | 글자 한 조각. `tone` (muted · subtle · accent · ok · warn · danger), `size` (xs · sm · md · lg), `mono`, `strong`, `truncate` (넘치면 … + 전체는 title) |
| `Code` · `CodeBlock` · `Kbd` | 줄 안 코드 / 여러 줄 코드 (SQL 미리 보기 · DDL) / 단축키 표기 |
| `Row` · `Spacer` · `Divider` · `ToolbarSep` | 가로 배치 (gap 1~6) / 남는 폭 / 구분선 (가로 · 세로) |
| `Button` | `variant`: default · primary (창마다 하나, 맨 오른쪽) · danger · ghost · link. `size`: md 26px · sm 22px. 글자는 동사 |
| `IconButton` · `ToolButton` | 아이콘만 (title 필수, `tone` ok · danger, `active`) / 도구 막대 글자 버튼 (`active` · `accent`) |
| `Badge` · `Avatar` · `Swatch` · `ProgressBar` | 개수 · 상태 (`tone`, 정해진 색은 `color`) / 이니셜 사각형 / 색 견본 / 진행 막대 (`pct` null = 모름) |
| `TextInput` · `Select` · `TextArea` · `Checkbox` · `Radio` | 입력 (`full` · `grow`) / 고르기 / 여러 줄 (고정폭) / 체크 (`indeterminate`) / 하나 고르기 |

## GUI 분자 (`molecules.tsx`)

| 조각 | 규칙 |
|---|---|
| `Field` · `RadioGroup` · `FormSection` · `Hint` · `ColorPicker` | 라벨 + 칸 (힌트 · 오류는 아래) / 선택지마다 설명 / 구역 제목 / 작은 회색 설명 / 색 고르기 |
| `SearchField` | ⌕ + 입력 (+ 불러오는 중 ◌). `bare` = 팝오버 머리처럼 테두리 없이 크게 |
| `Tabs` · `SectionTitle` · `KeyHints` | 구역 바꾸기 (`size` sm) / 목록 · 창 안의 작은 제목 / "↩ Open · ⇥ Next tab" |
| `Notice` · `EmptyState` | 안내 · 경고 · 오류 한 덩어리 / 비어 있을 때 |
| `ListItem` | 목록 한 줄: [앞(체크)] [색 띠] [아이콘] 제목 설명 ……… 오른쪽, 두 번째 줄 (`subtitle`), `selected` · `dim` · `mono` |
| `TreeRow` | 트리 한 줄: 들여쓰기 ▸ [앞] 아이콘 이름 [배지] 설명 ……… 오른쪽 |
| `DetailRow` | 코드 + 오른쪽 값 (행 수 · pid · 버전), 값 `tone` |
| `MenuItem` · `MenuSeparator` · `MenuLabel` | 메뉴 한 줄 (이름 ……… 단축키 · ▸) / 구분선 / 흐린 설명 줄 |
| `TabItem` · `RailItem` · `StatusItem` | 닫을 수 있는 탭 (편집기 · 결과, 📌 고정) / 왼쪽 세로 막대 칸 / 상태 줄 칸 |

## GUI 유기체 (`organisms.tsx`) · 결과 표 (`grid.tsx`)

| 조각 | 규칙 |
|---|---|
| `Toolbar` | 높이 30px, 아이콘 간격 2px, 묶음 사이는 `ToolbarSep` |
| `Modal` · `Overlay` · `Popover` | 대화상자 (`size` sm 420 · md 520 · lg 760 · xl 900, Esc · 바깥 클릭으로 닫힘, 버튼은 오른쪽: 취소 → 주 버튼) / 팝업 뒤 바탕 / 떠 있는 판 (머리 · 목록 · 아래 안내) |
| `confirm()` · `ConfirmHost` | 앱 안의 확인 창. `detail` = DetailList, `danger` 면 빨간 버튼 + 기본 포커스 Cancel. 버튼은 동작 이름 (Delete · Run …) |
| `Menu` | 우클릭 (`at`) · 드롭다운. 항목: 동작 · 구분선 · 설명 줄 (`heading`) · 하위 메뉴. 바깥 클릭 · ⎋ 로 닫힘 |
| `List` · `ListGroup` · `DetailList` | 목록 (`boxed`, 비면 `empty`) / 목록 안 묶음 제목 / 상세 목록 |
| `Table` · `Th` · `Td` | 작은 표 (`num` · `mono` · `muted`) |
| `FormGrid` | 긴 설정 창: `<label>` + 칸, 줄 전체는 `FormSection` |
| `GridHeaderRow` · `GridHeaderCell` · `GridRow` · `RowNumberCell` · `GridCell` · `CellEditor` | 결과 표 칸 (상태: 선택 · 커서 · 바뀜 · 찾기 · 추가 · 지움 · null · 숫자). 가상 스크롤 · 선택은 ResultGrid |

아이콘 (`icons.tsx`) 은 DB 종류별 고유색을 쓴다. 테마를 따라야 하는 것 (펼침 화살표) 은 `currentColor`.

## TUI (`crates/tui/src/kit`)

GUI 와 같은 이름 · 같은 색. 화면 코드에 `Color::Rgb(…)` 나 손으로 만든 줄을 쓰지 않는다.

| 단계 | 조각 |
|---|---|
| 원자 (`atoms.rs`) | 색 토큰 `ACCENT` `DIM` `SEL` `SEL_BLUR` `SURFACE` `POPUP_BG` `OK` `WARN` `DANGER` `NUMBER` `C_DB` … · `selected(focused)` · `key` · `muted` · `icon` · `swatch` · `badge` · `fit_spans` (정확히 폭만큼, 끝에 걸린 두 칸 글자 처리) · `clear` |
| 분자 (`molecules.rs`) | `Row` (들여쓰기 · ▸ · 앞 조각 · 이름 · 설명 · 오른쪽 · 색 · 선택 → `line()` / `fit(skip, width)`) · `key_row` · `field_row` · `key_hint` · `key_hints` · `section` |
| 유기체 (`organisms.rs`) | `panel(title, focused)` · `centered` · `popup` · `popup_danger` · `confirm_lines` (행 수 색: 0 흐리게 · 1~99 WARN · 100+ DANGER) · `wrapped_height` (접힌 줄까지 높이) |

TUI 화면을 바꾸면 `pnpm tui-snapshots` 로 Storybook 장면을 다시 그려 같이 커밋한다 (`crates/tui/src/kit_snapshots.rs`).

## 글

- 제목은 Title Case ("Back Up Database"), 버튼은 동사.
- 단축키 표기는 macOS 에서 ⌘ ⌥ ⌃ ⇧ (Alt+ / Ctrl+ 는 Windows · Linux 만).
- 넘치는 글은 줄임표 + `title` 로 전체를 보인다 (잘린 채 두지 않는다).
