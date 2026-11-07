# 설정

[English](../settings.md) | 한국어

GUI **Settings** (`⌘,`, 또는 맨 왼쪽 워크스페이스 막대 아래의 ⚙ 버튼) 에는 **Appearance**, **Keymap**, **Passwords** 세 쪽이 있습니다. GUI 와 TUI 가 함께 쓰는 설정은 설정 폴더의 `settings.json` 에 저장됩니다 ([폴더 위치](getting-started.md#설정이-저장되는-곳)). TUI 에는 설정 창이 없습니다: 옵션은 `⌃k` (Find Action) 에 있고, 글꼴은 터미널의 글꼴을 씁니다 ([터미널에서 쓰는 TUI › 글꼴](tui.md#글꼴) 참고).

## Appearance

| 설정 | 하는 일 | TUI 와 공유 |
|---|---|---|
| Theme | Dark, Light, High contrast, Dracula, One Dark, Solarized Dark, Solarized Light 또는 사용자 테마, 또는 어두운 테마와 밝은 테마를 하나씩 고르는 **Sync with OS** ([아래](#테마)). 빠른 전환: `` ⌃` `` | 예 (TUI **⌃k › Theme…**) |
| Editor font | 쿼리 콘솔 편집기의 글꼴과 크기. 크기 8–32 px (기본 13). `⌃⇧.` / `⌃⇧,` 로도 바꿀 수 있고, Settings 에 바뀐 크기가 보입니다 | 아니요 (TUI 는 터미널 글꼴을 씀) |
| Grid font | 결과 · 테이블 그리드의 글꼴과 크기. 크기 9–16 px (기본 12): 행 높이는 그대로라 더 크면 글자가 잘립니다. 크기에 맞춰 열이 넓어집니다 | 아니요 (TUI 는 터미널 글꼴을 씀) |
| Alignment | 결과 표에서 칸 안의 값을 어디에 둘지, 값 종류마다 (아래) | 예 (TUI **⌃k › Grid Alignment…**) |
| Updates | 앱을 켤 때 새 버전 확인, **Check Now** | — |
| Channel | stable 또는 beta 릴리스 ([업데이트](updates.md)) | 예 |

### 글꼴

글꼴은 바꾸는 즉시 열려 있는 모든 콘솔과 그리드에 적용됩니다.

**글꼴 이름**: 이 컴퓨터에 설치된 글꼴 목록에서 고릅니다. **Monospaced only** (기본으로 켜짐) 는 고정폭이 아닌 글꼴을 숨깁니다. 끄면 설치된 어떤 글꼴이든 고를 수 있습니다. 입력하면 목록을 검색하고 (닫힌 입력란에서 입력해도 열립니다), `↑` `↓` 로 움직이고, `↩` 로 고르고, `⎋` 로 목록을 닫습니다. 입력란의 글자는 고른 글꼴로 보입니다. 목록 맨 위의 **Default monospace** 는 JetBrains Mono, 없으면 Menlo 입니다. 목록은 열 때마다 다시 읽으므로, Qurry 가 켜져 있는 동안 설치한 글꼴도 다시 시작하지 않고 바로 보입니다. macOS 와 Linux (`fc-list`) 에서는 설치된 글꼴을 보여 주고, 그 밖에서는 기본 글꼴과 저장된 글꼴만 나옵니다.

저장된 글꼴이 설치되어 있지 않아도 (그 뒤에 지웠거나 다른 컴퓨터에서 고른 글꼴) 화면이 깨지지 않습니다: Qurry 는 기본 고정폭 글꼴을 쓰고, 입력란과 목록 맨 위에 `<이름> (not installed)` 로 보여 줍니다. 다른 글꼴을 고르면 바뀝니다.

**Add Font File…** (목록 맨 아래, DataGrip 에는 없음) 는 컴퓨터에 설치하지 않고 글꼴 파일을 씁니다: 20 MB 이하의 `.ttf`, `.otf`, `.woff`, `.woff2` 파일을 고르면 Qurry 가 설정 폴더 안의 `fonts` 폴더로 복사하고 그 입력란의 글꼴로 고릅니다. 복사본 이름은 파일 이름을 따릅니다 (글자, 숫자, `-`, `_` 는 그대로, 나머지는 공백). 같은 이름의 글꼴을 이미 추가했다면 덮어쓰지 않고 ` 2`, ` 3`… 을 붙입니다. 실제로 글꼴이 아닌 파일 (확장자가 아니라 파일 앞부분을 봅니다) 이나 글꼴로 불러올 수 없는 파일 (손상되었거나 덜 받은 파일) 은 이유와 함께 거절합니다. 추가한 글꼴은 두 입력란 모두에서 **Default monospace** 바로 다음에 `custom` 표시와 함께 나오고, **Monospaced only** 가 켜져 있어도 보입니다. 설치된 글꼴과 이름이 같으면 목록에서 그 글꼴 대신 나옵니다. 그 줄의 `−` 버튼은 (확인을 받은 뒤) 글꼴을 지웁니다: 복사본만 지우고 원래 파일은 그대로 두며, 그 글꼴을 쓰던 입력란은 **Default monospace** 로 돌아갑니다 (같은 이름의 글꼴이 설치되어 있으면 그대로 두고 설치된 글꼴로 보입니다). 추가한 글꼴은 Qurry 를 켤 때마다 불러옵니다. TUI 는 터미널 글꼴을 쓰므로 추가한 글꼴이 적용되지 않습니다.

글꼴은 이 컴퓨터의 GUI 가 따로 보관합니다 (설정 폴더가 아님. 추가한 글꼴 파일만 그 안의 `fonts` 폴더에 복사됩니다). 그래서 TUI 에는 영향이 없습니다.

### 테마

기본 테마는 일곱 가지입니다: **Dark**, **Light**, **High contrast**, 그리고 **Dracula**, **One Dark**, **Solarized Dark**, **Solarized Light**. 테마 하나가 모든 색을 정합니다: 패널, 편집기 (SQL, 그리고 Value Editor 의 JSON), 결과 표와 그 안의 고친 · 추가한 · 지울 행, 찾기 결과, NULL, Data Source 배경색과 운영 (Production) 띠, 그리고 TUI. 기본 테마는 모두 글자가 4.5:1 이상, 흐린 글자 · NULL · 아이콘 · 강조색이 3:1 이상의 대비 (WCAG) 를, 그 색이 그려지는 모든 바탕에서 지킵니다: 편집기와 그 현재 줄, 패널, 결과 표의 홀수 행, 마우스를 올린 곳, 선택, 운영 배경색. 원래 팔레트가 이를 못 지키는 곳 (Solarized 의 글자와 흐린 글자, Dracula 와 One Dark 의 흐린 글자, One Dark 의 주석, 일부 상태 색) 은 같은 색조에서 조금 밝게 또는 어둡게 바꿨습니다.

**Settings › Appearance › Theme**, **Quick Switch Theme** (`` ⌃` ``, 사용자 테마도 나옵니다), 또는 Find Action (`Theme: …`) 에서 고릅니다. **Sync with OS** 를 켜면 시스템이 어두운 모드일 때의 테마와 밝은 모드일 때의 테마를 고르는 칸이 둘 더 나옵니다 (어두운 · 고대비 테마 중 하나와 밝은 테마 중 하나, 사용자 테마 포함). 시스템이 바뀌면 Qurry 도 바뀝니다. 어느 앱에서도 테마를 고르기 전에는 GUI 는 Dark, TUI 는 Sync with OS (터미널은 밝을 수도 어두울 수도 있으므로) 로 그립니다. 한쪽에서 고르면 두 앱 모두 그 테마를 씁니다.

테마는 설정 폴더의 `settings.json` 에 저장되고 (`"theme"`, `"themeSync"`, `"customThemes"`), TUI 도 같은 테마를 씁니다 (**⌃k › Theme…**. TUI 의 Sync with OS 는 터미널 배경을 따릅니다. [TUI › 테마](tui.md#테마) 참고). 0.3.1 까지 GUI 는 테마를 앱 안에 따로 보관했습니다: 0.3.2 를 처음 켤 때 그 테마를 `settings.json` 으로 옮깁니다. 단, TUI 가 이미 저장한 테마가 있으면 그것을 씁니다. 옮기지 못하면 상태 표시줄에 알려 주고 그번 실행에서는 그 테마를 씁니다. Settings 에서 다시 고르면 저장됩니다.

**Edit Colors…** 로 사용자 테마를 만듭니다. **Based on** 에서 아무 테마로나 시작합니다 (기본 테마, 또는 고칠 사용자 테마). 기본 색 (배경, 패널, 글자, 흐린 글자, 강조색, 선택, 문법 색, 운영 배경색) 이 먼저 나오고, **Advanced colors** 에 나머지 (행 번호, 테두리, 아이콘, 표의 상태 색, 초록 배경색, 운영 띠, 상태 색) 가 있습니다. 운영 배경색과 띠는 **Red** 로 칠한 Data Source 에, 초록 배경색은 **Green** 으로 칠한 Data Source 에 쓰이고, 다른 Data Source 색은 테마의 패널 색에 섞어 씁니다. 색마다 색 고르기 창과 `#rrggbb` 값이 있고 (입력 중인 값은 색이 되는 순간 적용되며, 색이 아니면 표시만 하고 적용하지 않습니다), ↺ 는 기반 테마의 색으로 되돌립니다. 고치는 동안 창 전체가 새 색으로 그려지고, 미리보기에는 탐색기, 탭, 콘솔, 모든 상태의 결과 표, JSON 값, 알림이 나옵니다. 색 이름에 마우스를 올리면 그 색을 쓰는 곳에 테두리가 표시됩니다. 그려지는 바탕 중 어디에서든 대비 기준에 못 미치는 색은 그 옆에 가장 많이 못 미치는 바탕과 함께 (`⚠ 2.1:1 on Background`), 그리고 위쪽 제목에 경고가 나오고, 저장하면 고른 그대로 저장됩니다: 대신 바꾸지 않습니다. **Save as new theme** 은 입력한 이름으로 추가하고 그 테마로 바꾸며 (Sync with OS 가 켜져 있었으면 꺼지고, 알림에 그렇게 나옵니다), **Save** 는 시작한 사용자 테마를 덮어쓰고 (Sync with OS 는 켜진 채로 둡니다), **Delete** 는 지웁니다 (그 테마를 고른 곳은, Sync with OS 를 포함해, 기반 테마로 돌아갑니다). 저장하지 않은 색이 있으면 나가기 전에 묻고, 그 색을 바꿔 버리는 **Based on** 변경과 **Import…** 도 먼저 묻습니다.

**Export…** 는 테마를 작은 JSON 파일로 쓰고, **Import…** 는 파일을 편집기로 읽어 옵니다 (확인한 뒤 저장하세요):

```json
{ "qurryTheme": 1, "name": "Night shift", "base": "dracula", "colors": { "bg": "#1d1e26", "accent": "#9580ff" } }
```

`base` 는 기본 테마, `colors` 는 그 테마에서 바꾼 색만, 이 이름들로 씁니다 (편집기 순서: `bg`, `panel`, `panel2`, `odd`, `hover`, `border`, `borderStrong`, `text`, `muted`, `nullText`, `icon`, `accent`, `link`, `onAccent`, `sel`, `keyword`, `string`, `number`, `comment`, `function`, `jsonKey`, `gridNum`, `changed`, `added`, `deleted`, `match`, `prodTint`, `prodStripe`, `tint`, `ok`, `warn`, `danger`). 16 KB 보다 크거나, 이 JSON 이 아니거나, 다른 키, 모르는 색 이름이나 기반 테마, `#rrggbb` 가 아닌 색이 있는 파일은 이유와 함께 거절합니다.

0.3.2 에서 Dark, Light, High contrast 는 대비 기준에 못 미치던 곳만 조금 바뀌었습니다: 강조색 위의 글자 (주 버튼. High contrast 는 이제 검정), 운영 띠, Dark 의 흐린 글자와 NULL 회색, Light 의 강조색 · 링크 파랑, NULL 회색, 운영 · 초록 배경색, Light 의 오류 빨강.

### 결과 표의 정렬 (Alignment)

칸은 담긴 값의 종류에 따라 정렬됩니다. 종류마다 **Left**, **Center**, **Right** 를 고릅니다 (NULL 은 **Column type** 도 있습니다).

| 종류 | 어떤 값 | 기본값 |
|---|---|---|
| String | 글자, `uuid`, enum, 배열, range, Qurry 가 모르는 타입 | Left |
| Number | 정수, `numeric` / `decimal`, 부동소수점 (Oracle `BINARY_FLOAT` / `BINARY_DOUBLE` 도), `money`, `oid` (드라이버가 글자로 보내도) | Right |
| Date / time | `date`, `time`, `timestamp`, `interval`, `datetime` | Left |
| Boolean | `bool` / `boolean` | Left |
| NULL | `<null>` (추가 중인 행의 `<default>` 도) | Column type |
| JSON / binary | `json` / `jsonb`, `bytea`, `blob`, `binary` / `varbinary` | Left |

기본값인 **Column type** 에서는 `NULL` 이 그 컬럼의 값과 같은 쪽에 놓입니다. 글자, 날짜 / 시간, boolean, JSON 컬럼에서는 왼쪽, 숫자 컬럼에서는 오른쪽이라 위아래 값과 줄이 맞습니다. 타입이 없는 컬럼 (`sum(x)` 같은 SQLite 식, MongoDB) 은 처음 나오는 `NULL` 아닌 값을 따릅니다. 그 종류의 정렬을 바꿨다면 `NULL` 도 따라갑니다. **Left** 나 **Right** 를 고르면 컬럼과 상관없이 모든 `NULL` 이 그쪽에 놓입니다. 0.3.0 에서는 모든 `NULL` 이 오른쪽이었습니다. 그대로 쓰려면 **NULL** 을 **Right** 로 바꾸세요. 0.3.0 전에는 왼쪽 (**Left**) 이었습니다.

JSON 칸 (`json`, `jsonb` 와 다른 드라이버의 JSON 타입) 은 DataGrip 처럼 색으로 구분됩니다: 키, 문자열, 숫자, `true` / `false` / `null`, 구두점. 한 줄로 보이는 것은 같습니다. 바이너리 칸은 색이 없습니다. TUI 도 터미널 색으로 똑같이 칠하고, `NO_COLOR` 에서는 칠하지 않습니다.

TUI 결과 표도 같은 종류와 같은 설정을 씁니다. TUI 에서는 **⌃k › Grid Alignment…** 에서 바꿉니다 (종류를 고른 뒤 Left, Center, Right. NULL 은 Column type 도). TUI 는 켤 때와 **⌃k › Grid Alignment…** 를 열 때 `settings.json` 을 읽으므로, GUI 에서 정렬을 바꾼 뒤에는 그 명령을 열거나 TUI 를 다시 켜면 반영됩니다. `settings.json` 에는 바꾼 종류만 저장됩니다 (예: `"gridAlign": { "null": "right" }`). 기본값으로 되돌린 종류는 지워집니다. 0.3.0 이 저장한 `"null": "left"` (또는 `"center"`) 는 그대로 적용됩니다. Right 는 0.3.0 의 기본값이라 저장되지 않았으므로, Right 를 쓰던 분은 **Right** 를 다시 고르세요.

### 결과 표의 컬럼 너비

컬럼은 처음에 이름과 앞쪽 값들이 들어갈 만큼 (상한까지) 넓어서, 긴 값은 `…` 로 잘립니다. 너비를 바꾸려면:

- 컬럼 헤더의 오른쪽 가장자리를 **드래그** 합니다. 가장자리를 **더블클릭** 하면 내용에 맞춥니다: 컬럼 이름과 지금까지 불러온 행 중 가장 긴 값.
- 컬럼 헤더를 **오른쪽 클릭**: **Fit Column**, **Fit All Columns**, **Reset Widths** (모든 컬럼을 계산된 너비로), **Narrow Column**, **Widen Column**.
- 셀이나 컬럼 헤더를 고른 상태에서 `⌃⌥←` / `⌃⌥→` 는 현재 컬럼을 4 글자씩 좁히고 / 넓힙니다 (**Settings › Keymap** 의 *Data grid* 에서 키를 바꿀 수 있습니다).

TUI 에서는 `<` / `>` 가 현재 컬럼을 좁히고 / 넓히고 `=` 가 내용에 맞춥니다 (키 안내 줄에 나오고, `F1` 창의 *Data grid keys* 에서 바꿀 수 있습니다). **⌃k › Fit All Columns** 와 **⌃k › Reset Column Widths** 는 모든 컬럼에 적용됩니다.

테이블 탭은 너비를 테이블마다 (데이터 소스 · 데이터베이스 · 스키마 · 테이블) `settings.json` 의 `"columnWidths"` 에 기억하므로, GUI 와 TUI 가 같은 너비를 보여 줍니다. 단위는 글자 수이고, GUI 는 그리드 글꼴로, TUI 는 그만큼의 칸으로 그립니다. 넓은 문자 (한글, 한자, 일본어, 이모지) 는 터미널이 그리듯 2 글자로 세므로, 내용에 맞춘 컬럼은 두 앱 모두에서 값 전체를 보여 줍니다. 테이블 탭은 열 때마다 저장된 너비를 읽으므로, 다른 앱에서 바꾼 너비는 그 테이블을 다시 열면 보입니다. 너비를 바꾸지 않은 컬럼은 계산된 너비 그대로이고, **Reset Widths** 는 그 테이블의 너비를 지웁니다. 설정 파일에 쓸 수 없으면 새 너비는 탭을 닫을 때까지 적용됩니다. 콘솔 결과는 다시 실행할 때까지 너비를 유지하고, 이름이 같은 컬럼 (조인의 `id` 컬럼 두 개) 은 각각 따로 너비가 바뀝니다. 전치 보기 (**View as › Transpose**) 는 자기 레이아웃을 그대로 씁니다.

## Keymap

GUI 단축키 전체와 바꾼 키: [GUI 단축키](shortcuts-gui.md). 바꾼 단축키는 `keymap.json` 에 저장됩니다. TUI 키는 TUI 의 `F1` 창에서 바꿉니다.

## Passwords

저장된 비밀번호의 키를 어디에 둘지 (설정 폴더 또는 시스템 키체인): [시작하기 › 설정이 저장되는 곳](getting-started.md#설정이-저장되는-곳).

---

[← 사용설명서](README.md)
