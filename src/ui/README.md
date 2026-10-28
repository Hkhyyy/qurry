# UI 규격 (GUI)

화면은 이 폴더의 토큰과 컴포넌트로만 만든다. 새 화면에서 `<button className=…>`, `<div className="modal">`, 숫자 px 를 직접 쓰지 않는다.
모양을 바꿀 일이 있으면 여기 (tokens.css · ui.css · index.tsx) 를 고치고, 화면 코드는 건드리지 않는다.

미리 보기: `pnpm storybook` → http://localhost:6006 (GUI 컴포넌트 · TUI 화면, 위쪽에서 테마 바꾸기). 스토리는 `src/ui/stories/`.

## 토큰 (`tokens.css`)

| 종류 | 토큰 | 값 (dark) | 쓰는 곳 |
|---|---|---|---|
| 바탕 | `--ui-bg` | #1e1f22 | 편집기, 표, 입력칸 |
| | `--ui-surface` | #2b2d30 | 패널, 도구 막대, 대화상자, 메뉴 |
| | `--ui-surface-2` | #26282b | 패널 안의 한 단계 낮은 면 (목록, 미리 보기) |
| | `--ui-hover` | #33353a | 줄 · 버튼 위에 올렸을 때 |
| | `--ui-selected` | #2e436e | 고른 줄 |
| 선 | `--ui-border` | #393b40 | 구역 나누기 |
| | `--ui-border-strong` | #4e5157 | 입력칸 · 버튼 테두리 |
| 글자 | `--ui-text` | #dfe1e5 | 본문 |
| | `--ui-text-muted` | #868a91 | 설명, 힌트, 단축키 |
| | `--ui-text-on-accent` | #ffffff | 강조색 위 글자 |
| 색 | `--ui-accent` | #3574f0 | 주 버튼, 포커스, 링크 |
| | `--ui-danger` | #f75464 | 지우기 · 운영 경고 |
| | `--ui-warn` | #e5a64b | 주의 (커밋 안 한 변경 등) |
| | `--ui-ok` | #5fb865 | 성공, 실행 |
| 간격 | `--ui-sp-1` … `--ui-sp-6` | 2 · 4 · 8 · 12 · 16 · 24px | padding · gap · margin 은 이 값만 |
| 둥글기 | `--ui-radius-sm` / `-md` / `-lg` | 4 · 6 · 10px | 버튼·입력칸 / 메뉴·배지 / 대화상자 |
| 글자 크기 | `--ui-fs-xs` / `-sm` / `-md` / `-lg` | 11 · 12 · 13 · 14px | 배지 / 보조 글·도구 막대 / 본문 / 대화상자 제목 |
| 높이 | `--ui-h-sm` / `-md` / `-lg` | 22 · 26 · 30px | 작은 버튼 / 버튼·입력칸 / 도구 막대·패널 머리 |
| 그림자 | `--ui-shadow-pop` / `-modal` | | 메뉴·팝업 / 대화상자 |
| 쌓기 | `--ui-z-modal` 100 · `-menu` 200 · `-popover` 300 · `-dialog` 400 | | 확인 창은 늘 맨 위 |

라이트 · 고대비 테마는 같은 이름에 다른 값만 준다.

## 컴포넌트 (`index.tsx`)

| 컴포넌트 | 규칙 |
|---|---|
| `Button` | `variant`: `default` · `primary` (창마다 하나, 맨 오른쪽) · `danger` · `ghost` · `link`. `size`: `md` (26px) · `sm` (22px). 글자는 동사로 ("Save", "Run", "Back Up") |
| `IconButton` | 아이콘만. `title` 필수 (마우스를 올리면 이름과 단축키). `tone="ok"/"danger"` 는 실행·멈춤처럼 의미가 있을 때만 |
| `Toolbar` · `ToolbarSep` · `Spacer` | 높이 30px, 아이콘 간격 2px, 묶음 사이는 `ToolbarSep` |
| `Badge` | 개수 · 상태 표시. `tone`: `neutral` · `accent` · `warn` · `danger`. 눌러지면 `onClick` |
| `Modal` | 제목, 본문, 아래 버튼 줄. `size`: `sm` 420 · `md` 520 · `lg` 760 · `xl` 900px. Esc 와 바깥 클릭으로 닫힘, 버튼은 오른쪽 정렬, 취소 → 주 버튼 순서 |
| `confirm()` | 앱 안의 확인 창 (macOS 기본 창 쓰지 않음). `detail` 로 문장 · 행 수 같은 표, `danger` 면 OK 가 빨강 |
| `Field` · `TextInput` · `Select` · `Checkbox` · `RadioGroup` | 폼은 왼쪽 라벨 (120px) + 오른쪽 칸. 힌트는 칸 아래 작은 회색 글 |
| `Tabs` | 대화상자 · 화면 안의 구역 바꾸기 (History / Snippets 같은 것) |
| `Notice` | 안내 · 경고 · 오류 한 덩어리 (`tone`) |
| `EmptyState` | 비어 있을 때 한두 줄 + 할 일 버튼 |

## 글

- 제목은 Title Case ("Back Up Database"), 버튼은 동사.
- 단축키 표기는 macOS 에서 ⌘ ⌥ ⌃ ⇧ (Alt+ / Ctrl+ 는 Windows · Linux 만).
- 넘치는 글은 줄임표 + `title` 로 전체를 보인다 (잘린 채 두지 않는다).

## TUI (`crates/tui/src/kit.rs`)

TUI 도 같은 이름 · 같은 색을 쓴다. 화면 코드에 `Color::Rgb(…)` 를 직접 쓰지 않는다.

| 토큰 · 조각 | 쓰는 곳 |
|---|---|
| `ACCENT` · `DIM` | 포커스 테두리 · 키 이름 / 흐린 글자 · 포커스 없는 테두리 |
| `SEL` · `SEL_BLUR` · `selected(focused)` | 고른 줄 바탕 |
| `SURFACE` · `POPUP_BG` | 상태 줄 · 결과 머리 / 팝업 바탕 |
| `OK` · `WARN` · `DANGER` | 성공 · 주의 · 위험 (운영 확인의 행 수: 0 흐리게, 1~99 WARN, 100+ DANGER) |
| `panel(title, focused)` | 모든 칸의 테두리 |
| `popup(f, r, title)` · `popup_danger(…)` | 팝업 자리 비우기 + 테두리 (위험한 확인은 빨강) |
| `badge(text, color)` | 상태 줄의 `PROD` · `READ-ONLY` |
| `key_hint(key, label)` | "y / Enter yes" 처럼 키는 강조색 |
| `fit_spans(spans, skip, width)` | 한 줄을 정확히 폭만큼 (두 칸 글자가 끝에 걸려도 줄 색이 끊기지 않게) |

미리 보기: Storybook 의 **TUI** — `pnpm tui-snapshots` 가 실제 TUI 코드로 장면을 그려 `src/ui/stories/tui-snapshots.json` 에 남긴다 (`crates/tui/src/kit_snapshots.rs`). TUI 화면을 바꾸면 다시 돌려 함께 커밋한다.
