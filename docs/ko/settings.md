# 설정

[English](../settings.md) | 한국어

GUI **Settings** (`⌘,`) 에는 **Appearance**, **Keymap**, **Passwords** 세 쪽이 있습니다. GUI 와 TUI 가 함께 쓰는 설정은 설정 폴더의 `settings.json` 에 저장됩니다 ([폴더 위치](getting-started.md#설정이-저장되는-곳)). TUI 에는 설정 창이 없습니다: 옵션은 `⌃k` (Find Action) 에 있고, 글꼴은 터미널의 글꼴을 씁니다 ([터미널에서 쓰는 TUI › 글꼴](tui.md#글꼴) 참고).

## Appearance

| 설정 | 하는 일 | TUI 와 공유 |
|---|---|---|
| Theme | Dark, Light, High contrast, 또는 **Sync with OS**. 빠른 전환: `` ⌃` `` | 아니요 (TUI 는 터미널 색을 따름) |
| Editor font | 쿼리 콘솔 편집기의 글꼴과 크기. 크기 8–32 px (기본 13). `⌃⇧.` / `⌃⇧,` 로도 바꿀 수 있고, Settings 에 바뀐 크기가 보입니다 | 아니요 (TUI 는 터미널 글꼴을 씀) |
| Grid font | 결과 · 테이블 그리드의 글꼴과 크기. 크기 9–16 px (기본 12): 행 높이는 그대로라 더 크면 글자가 잘립니다. 크기에 맞춰 열이 넓어집니다 | 아니요 (TUI 는 터미널 글꼴을 씀) |
| Alignment | 결과 표에서 칸 안의 값을 어디에 둘지, 값 종류마다 (아래) | 예 (TUI **⌃k › Grid Alignment…**) |
| Updates | 앱을 켤 때 새 버전 확인, **Check Now** | — |
| Channel | stable 또는 beta 릴리스 ([업데이트](updates.md)) | 예 |

### 글꼴

글꼴은 바꾸는 즉시 열려 있는 모든 콘솔과 그리드에 적용됩니다.

**글꼴 이름**: 이 컴퓨터에 설치된 글꼴 목록에서 고릅니다. **Monospaced only** (기본으로 켜짐) 는 고정폭이 아닌 글꼴을 숨깁니다. 끄면 설치된 어떤 글꼴이든 고를 수 있습니다. 입력하면 목록을 검색하고 (닫힌 입력란에서 입력해도 열립니다), `↑` `↓` 로 움직이고, `↩` 로 고르고, `⎋` 로 목록을 닫습니다. 입력란의 글자는 고른 글꼴로 보입니다. 목록 맨 위의 **Default monospace** 는 JetBrains Mono, 없으면 Menlo 입니다. 목록은 열 때마다 다시 읽으므로, Qurry 가 켜져 있는 동안 설치한 글꼴도 다시 시작하지 않고 바로 보입니다. macOS 와 Linux (`fc-list`) 에서는 설치된 글꼴을 보여 주고, 그 밖에서는 기본 글꼴과 저장된 글꼴만 나옵니다.

저장된 글꼴이 설치되어 있지 않아도 (그 뒤에 지웠거나 다른 컴퓨터에서 고른 글꼴) 화면이 깨지지 않습니다: Qurry 는 기본 고정폭 글꼴을 쓰고, 입력란과 목록 맨 위에 `<이름> (not installed)` 로 보여 줍니다. 다른 글꼴을 고르면 바뀝니다.

테마와 글꼴은 이 컴퓨터의 GUI 가 따로 보관합니다 (설정 폴더가 아님). 그래서 TUI 에는 영향이 없습니다.

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

기본값인 **Column type** 에서는 `NULL` 이 그 컬럼의 값과 같은 쪽에 놓입니다. 글자, 날짜 / 시간, boolean, JSON 컬럼에서는 왼쪽, 숫자 컬럼에서는 오른쪽이라 위아래 값과 줄이 맞습니다. 그 종류의 정렬을 바꿨다면 `NULL` 도 따라갑니다. **Left** 나 **Right** 를 고르면 컬럼과 상관없이 모든 `NULL` 이 그쪽에 놓입니다. 0.3.0 에서는 모든 `NULL` 이 오른쪽이었습니다. 그대로 쓰려면 **NULL** 을 **Right** 로 바꾸세요. 0.3.0 전에는 왼쪽 (**Left**) 이었습니다.

JSON 칸 (`json`, `jsonb` 와 다른 드라이버의 JSON 타입) 은 DataGrip 처럼 색으로 구분됩니다: 키, 문자열, 숫자, `true` / `false` / `null`, 구두점. 한 줄로 보이는 것은 같습니다. 바이너리 칸은 색이 없습니다. TUI 도 터미널 색으로 똑같이 칠하고, `NO_COLOR` 에서는 칠하지 않습니다.

TUI 결과 표도 같은 종류와 같은 설정을 씁니다. TUI 에서는 **⌃k › Grid Alignment…** 에서 바꿉니다 (종류를 고른 뒤 Left, Center, Right. NULL 은 Column type 도). TUI 는 켤 때와 **⌃k › Grid Alignment…** 를 열 때 `settings.json` 을 읽으므로, GUI 에서 정렬을 바꾼 뒤에는 그 명령을 열거나 TUI 를 다시 켜면 반영됩니다. `settings.json` 에는 바꾼 종류만 저장됩니다 (예: `"gridAlign": { "null": "right" }`). 기본값으로 되돌린 종류는 지워집니다. 0.3.0 이 저장한 `"null": "left"` 나 `"null": "right"` 는 그대로 적용됩니다.

## Keymap

GUI 단축키 전체와 바꾼 키: [GUI 단축키](shortcuts-gui.md). 바꾼 단축키는 `keymap.json` 에 저장됩니다. TUI 키는 TUI 의 `F1` 창에서 바꿉니다.

## Passwords

저장된 비밀번호의 키를 어디에 둘지 (설정 폴더 또는 시스템 키체인): [시작하기 › 설정이 저장되는 곳](getting-started.md#설정이-저장되는-곳).

---

[← 사용설명서](README.md)
