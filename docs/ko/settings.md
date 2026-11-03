# 설정

[English](../settings.md) | 한국어

GUI 에서 `⌘,` 로 Settings 를 엽니다. TUI 에는 설정 창이 없습니다: 옵션은 `⌃k` (Find Action) 에 있고, 글꼴은 터미널의 글꼴을 씁니다 ([터미널에서 쓰는 TUI › 글꼴](tui.md#글꼴) 참고).

## Appearance

| 설정 | 하는 일 |
|---|---|
| **Theme** | Dark, Light, High contrast, 또는 **Sync with OS**. 빠른 전환: `` ⌃` `` |
| **Editor font** | 쿼리 콘솔 편집기의 글꼴과 크기. 크기 8–32 px (기본 13). `⌃⇧.` / `⌃⇧,` 로도 바꿀 수 있고, Settings 에 바뀐 크기가 보입니다 |
| **Grid font** | 결과 · 테이블 그리드의 글꼴과 크기. 크기 9–16 px (기본 12): 행 높이는 그대로라 더 크면 글자가 잘립니다. 크기에 맞춰 열이 넓어집니다 |
| **Updates**, **Channel** | 자동 업데이트 확인과 stable / beta 채널: [업데이트](updates.md) |

글꼴은 바꾸는 즉시 열려 있는 모든 콘솔과 그리드에 적용됩니다.

**글꼴 이름**: 목록에는 이 컴퓨터에 설치된 고정폭 글꼴 (JetBrains Mono, Menlo, Fira Code, D2Coding, Cascadia Code 같은 흔한 것들) 이 나옵니다. 그 밖에 설치된 글꼴은 이름을 직접 입력하면 됩니다. 입력란의 글자는 고른 글꼴로 보입니다. 비워 두면 기본 글꼴 (JetBrains Mono, 없으면 Menlo) 입니다.

설치되지 않은 글꼴 (오타, 또는 그 뒤에 지운 글꼴) 이어도 화면이 깨지지 않습니다: Qurry 는 기본 고정폭 글꼴을 쓰고, Settings 에 `"<이름>" isn't installed, so the default monospace font is used.` 라고 알려 줍니다. 목록에서 글꼴을 고르거나 입력란을 비우면 기본으로 돌아갑니다.

테마와 글꼴은 이 컴퓨터의 GUI 가 따로 보관합니다 ([설정 폴더](getting-started.md#설정이-저장되는-곳) 가 아님). 그래서 TUI 에는 영향이 없습니다.

## Keymap

GUI 단축키 전체와 바꾼 키: [GUI 단축키](shortcuts-gui.md). TUI 키는 TUI 의 `F1` 창에서 바꿉니다.

## Passwords

저장된 비밀번호의 키를 어디에 둘지 (설정 폴더 또는 시스템 키체인): [시작하기 › 설정이 저장되는 곳](getting-started.md#설정이-저장되는-곳).

---

[← 사용설명서](README.md)
