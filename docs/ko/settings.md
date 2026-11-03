# 설정

[English](../settings.md) | 한국어

GUI **Settings** (`⌘,`) 에는 **Appearance**, **Keymap**, **Passwords** 세 쪽이 있습니다. GUI 와 TUI 가 함께 쓰는 설정은 설정 폴더의 `settings.json` 에 저장됩니다 ([폴더 위치](getting-started.md#설정이-저장되는-곳)).

## Appearance

| 설정 | 하는 일 | TUI 와 공유 |
|---|---|---|
| Theme | Dark, Light, High contrast, 또는 **Sync with OS**. 빠른 전환: `` ⌃` `` | 아니요 (TUI 는 터미널 색을 따름) |
| Alignment | 결과 표에서 칸 안의 값을 어디에 둘지, 값 종류마다 (아래) | 예 (TUI **⌃k › Grid Alignment…**) |
| Updates | 앱을 켤 때 새 버전 확인, **Check Now** | — |
| Channel | stable 또는 beta 릴리스 ([업데이트](updates.md)) | 예 |

### 결과 표의 정렬 (Alignment)

칸은 담긴 값의 종류에 따라 정렬됩니다. 종류마다 **Left**, **Center**, **Right** 를 고릅니다.

| 종류 | 어떤 값 | 기본값 |
|---|---|---|
| String | 글자, `uuid`, enum, 배열, range, Qurry 가 모르는 타입 | Left |
| Number | 정수, `numeric` / `decimal`, 부동소수점 (Oracle `BINARY_FLOAT` / `BINARY_DOUBLE` 도), `money`, `oid` (드라이버가 글자로 보내도) | Right |
| Date / time | `date`, `time`, `timestamp`, `interval`, `datetime` | Left |
| Boolean | `bool` / `boolean` | Left |
| NULL | `<null>` (추가 중인 행의 `<default>` 도) | Right |
| JSON / binary | `json` / `jsonb`, `bytea`, `blob`, `binary` / `varbinary` | Left |

숫자 컬럼의 `NULL` 이 위아래 숫자와 줄이 맞도록 `NULL` 의 기본값은 오른쪽입니다. 0.3.0 전에는 왼쪽이었습니다. 예전처럼 쓰려면 **NULL** 을 **Left** 로 바꾸세요.

TUI 결과 표도 같은 종류와 같은 설정을 씁니다. TUI 에서는 **⌃k › Grid Alignment…** 에서 바꿉니다 (종류를 고른 뒤 Left, Center, Right). TUI 는 켤 때와 **⌃k › Grid Alignment…** 를 열 때 `settings.json` 을 읽으므로, GUI 에서 정렬을 바꾼 뒤에는 그 명령을 열거나 TUI 를 다시 켜면 반영됩니다. `settings.json` 에는 바꾼 종류만 저장됩니다 (예: `"gridAlign": { "null": "left" }`). 기본값으로 되돌린 종류는 지워집니다.

## Keymap

단축키를 바꿉니다. [GUI 단축키](shortcuts-gui.md) 를 보세요. 바꾼 단축키는 `keymap.json` 에 저장됩니다.

## Passwords

비밀번호 파일의 키를 설정 폴더에 둘지 시스템 키체인에 둘지 고릅니다. [설정이 저장되는 곳](getting-started.md#설정이-저장되는-곳) 을 보세요.

---

[← 사용설명서](README.md)
