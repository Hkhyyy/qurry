# 터미널에서 쓰는 TUI

[English](../tui.md) | 한국어

`qurry-tui` 는 돌고 있는 터미널에 맞춥니다: 색을 몇 개 쓸 수 있는지, 배경이 밝은지, 창이 얼마나 큰지, tmux 나 터미널이 가져가는 키가 무엇인지. 글꼴까지 모두 환경을 따르고, 설정은 터미널이 배경색을 알려 주지 않을 때 고르는 테마 하나뿐입니다.

## 색

TUI 는 시작할 때 아래 환경 변수로 색 모드를 하나 고릅니다 (먼저 맞는 줄이 이깁니다):

| 환경 | 보이는 것 |
|---|---|
| `NO_COLOR` 가 무엇으로든 설정됨 ([no-color.org](https://no-color.org) 참고) | 색이 전혀 없음. 선택 (편집기와 입력 칸에서 선택한 글자도), 배지 (`PROD`, `READ-ONLY`), 셀 커서는 반전으로, 흐린 글자는 dim 으로, 포커스가 있는 패널은 굵은 테두리로 보입니다 |
| `COLORTERM=truecolor` 또는 `24bit`, `-direct` 로 끝나는 `TERM`, 또는 kitty, Alacritty, WezTerm, foot, Ghostty 의 `TERM` | 24비트 테마 그대로 (GUI 와 같은 색) |
| `256color` 가 들어간 `TERM` (예: `xterm-256color`, `tmux-256color`) | 각 색을 xterm 256색 팔레트에서 가장 가까운 색으로. 색조는 유지됩니다: 운영 Data Source 는 계속 빨간색 |
| 그 밖 (예: `TERM=xterm`, `screen`) | 터미널 기본 16색. 선택한 줄은 반전으로, 색이 칠해진 줄의 글자는 검정 (빨강·파랑·자주 줄은 흰색) 으로 — 어떤 터미널 테마에서도 읽힙니다 |

색이 꺼졌거나 16색뿐이면 시작할 때 상태 줄에 무엇을 바꾸면 되는지 나옵니다:

- `Colors off: unset NO_COLOR` — `unset NO_COLOR` 를 실행하고 (또는 셸 프로필에서 지우고) `qurry-tui` 를 다시 시작하세요.
- `16 colors: try COLORTERM=truecolor` — 터미널이 24비트 색을 보여 준다면 시작 전에 `export COLORTERM=truecolor`. 요즘 터미널 (iTerm2, kitty, WezTerm, Ghostty, Alacritty, Windows Terminal) 은 스스로 설정하지만, `ssh`, `sudo`, `tmux` / `screen` 안에서는 자주 빠집니다.

`ssh` 나 `sudo` 로 (그리고 아래 `tmux` 안에서) 실행하면 보통 `COLORTERM` 이 전달되지 않아 24비트 터미널에서도 256색이나 16색이 됩니다: 접속한 쪽에서 `export COLORTERM=truecolor` 를 하거나, `~/.ssh/config` 의 `SendEnv COLORTERM` 과 서버 `sshd_config` 의 `AcceptEnv COLORTERM` 으로 넘기세요 (sudo 는 `sudo --preserve-env=COLORTERM`).

### 밝은 테마, 어두운 테마

TUI 에는 어두운 팔레트 (어두운 터미널 배경용) 와 밝은 팔레트가 있습니다. **⌃k › Theme…** 에서 고릅니다:

| 테마 | 팔레트 |
|---|---|
| **Auto** (기본) | `COLORFGBG` 가 배경이 흰색이라고 하면 (마지막 숫자가 `7` 이나 `15`, 예: `COLORFGBG=0;15`) 밝게, 그 밖에는 (`COLORFGBG` 가 없을 때도) 어둡게 |
| **Dark** | 항상 어둡게 |
| **Light** | 항상 밝게 |

밝은 팔레트는 GUI 밝은 테마의 색을 씁니다: Data Source 색은 같은 색조의 아주 옅은 색이 되어 그 위의 아이콘과 흐린 글자도 잘 읽히고 (활성 탭이 더 진합니다. 옅은 색조가 없는 256색 · 16색은 색조가 남도록 더 진한 색을 씁니다), 선택·고친 셀·배지는 서로 구분되고, JSON 셀은 GUI 의 밝은 JSON 색을, 운영은 계속 빨간색을 씁니다. 위의 모든 색 모드에서 동작합니다: 256색은 가장 가까운 팔레트 색으로, 16색은 (밝은 색이 아닌) 보통 터미널 색으로, `NO_COLOR` 는 어두운 테마와 같은 규칙의 흑백 그대로입니다.

iTerm2, rxvt, Konsole 같은 터미널은 배경색에 맞춰 `COLORFGBG` 를 설정하지만, 많은 터미널 (kitty, WezTerm, Ghostty, Alacritty) 은 설정하지 않으므로 밝은 배경이라면 직접 **Light** 를 고르세요. 테마는 [설정 폴더](getting-started.md#설정이-저장되는-곳)의 `settings.json` 에 `"tuiTheme"` 으로 저장됩니다 (GUI 테마 설정은 따로입니다).

Auto 가 밝은 팔레트를 골랐으면 시작할 때 상태 줄에 `Light palette — ⌃k › Theme… › Dark to undo` 가 나옵니다. 예전 색을 그대로 쓰려면 **⌃k › Theme… › Dark** 를 고르세요. 이 안내가 `16 colors` 안내 대신 나오며, `NO_COLOR` 에서는 팔레트가 달라지는 것이 없으므로 `Colors off` 안내가 나옵니다.

색과 상관없이 보이는 것:

- **저장하지 않은 그리드 변경**: 줄 번호 옆에 `+` 는 추가한 줄, `-` 는 삭제한 줄 (취소선도), `~` 는 셀을 고친 줄; 고친 셀은 굵게.
- **범위 안의 셀 커서**: 반전을 쓰는 모드 (16색, `NO_COLOR`) 에서는 범위가 반전되고, 커서는 그 안에서 반전되지 않은 한 칸입니다 (스프레드시트처럼).
- **활성 탭**: 굵게. 16색이나 `NO_COLOR` 에서는 포커스가 있는 창의 탭이 반전으로(Data Source 색을 뒤집어 보여 주므로 운영 탭은 계속 빨간색), 창을 나눴을 때 다른 창에 보이는 탭은 밑줄로 보입니다.
- **콘솔에서 실행한 문장**: 줄 번호 칸, 문장의 첫 줄에 `✓` 성공, `✗` 실패, `…` 실행 중.

`qurry-tui --help` 에도 같은 규칙이 나옵니다.

## 글꼴

TUI 의 글자는 터미널이 그리므로 글꼴과 크기는 터미널의 것을 씁니다: Qurry 에는 TUI 글꼴 설정이 없습니다 (GUI 는 [Settings › Appearance](settings.md#appearance)). 고정폭 글꼴을 고르세요. TUI 는 글자를 칸 단위로 배치하고 테두리를 선 문자 (box-drawing) 로 그립니다. 대부분의 터미널에서 `⌘+` / `⌘-` 로 지금 창의 크기를 바꿀 수 있습니다.

| 터미널 | 글꼴 바꾸는 곳 |
|---|---|
| Terminal.app | *Settings › Profiles › Text › Font › Change…* (프로필마다) |
| iTerm2 | *Settings › Profiles › Text › Font* (프로필마다) |
| Ghostty | `~/.config/ghostty/config` 에 `font-family = "JetBrains Mono"`, `font-size = 13`. `⇧⌘,` 로 다시 읽기 |
| WezTerm | `~/.wezterm.lua` 에 `config.font = wezterm.font("JetBrains Mono")`, `config.font_size = 13.0`. 저장하면 적용 |
| Alacritty | `~/.config/alacritty/alacritty.toml` 의 `[font]` 에 `normal = { family = "JetBrains Mono" }`, `size = 13.0`. 저장하면 적용 |

## 작은 창

화면은 **40 × 10** 칸까지 그려집니다. 80 × 24 이상이면 넉넉하고, 작은 창에서는 `⌥0` 으로 탐색기를 숨기거나 `⌥[` 로 좁히세요.
값 패널 (`v`) 은 결과 영역이 60 칸 이상일 때만 보입니다. 더 좁으면 창을 키울 때까지 그리드가 너비를 다 씁니다.
대화 상자도 그 크기까지 쓸 수 있습니다: Data Source 폼이 다 들어가지 않으면 칸이 지금 있는 칸까지 스크롤되고 (아래 테두리에 `12/28` 처럼 몇 번째 칸인지 나옵니다) 키 안내는 아래에 남습니다 (그 위의 긴 테스트 메시지는 잘리고 `⋯` 로 끝납니다). 확인 창은 높이를 다 쓰고 첫 줄 · 질문 · 모든 `⚠` 경고 (`⚠ Production data source` 등) · `y / Enter` · `n / Esc` 를 남깁니다. 그 사이의 내용 (문장 · 컬럼 · 테이블) 은 가려지고, 가려진 줄 수와 그 문장들이 바꿀 행 수를 알려 줍니다 (`⋯ 4 lines hidden · 12,409 rows`). 답하기 전에 창을 키워 읽어 보세요. 질문과 경고조차 다 들어가지 않으면 키 안내 줄이 `Enlarge the window to answer` 로 바뀌고, 다 들어갈 때까지 `y` / `Enter` 는 아무 일도 하지 않습니다 (`n` / `Esc` 로는 취소됩니다).

40 × 10 보다 작으면 깨진 화면 대신 `Terminal too small` 과 지금 크기, 필요한 크기가 나옵니다. 잃는 것은 없습니다: 창을 키우면 모두 그대로 돌아옵니다. 그동안 키와 클릭은 아무것도 하지 않아 보이지 않는 것이 실행되거나 답해지지 않습니다; `⌃q` 로 끝내기만 되고, 그것도 답을 기다리는 것이 없을 때만입니다. 대화 상자가 열려 있으면 `A dialog is open: make the window larger to answer it`, 콘솔에 커밋하지 않은 변경이 있으면 `N console(s) have uncommitted changes: make the window larger to quit` 이 나옵니다. 창을 키운 뒤 대화 상자에 답하거나, `⌃q` 를 누르고 변경을 되돌릴지 고르세요.

## tmux 와 다른 멀티플렉서

- **`⌃b` (Go to DDL) 는 tmux 의 기본 prefix 입니다.** Qurry 는 DataGrip 처럼 이 키를 그대로 둡니다. tmux 에서는 `⌃b` 를 두 번 누르세요: tmux 가 `⌃b` 하나를 Qurry 에 넘깁니다. 또는 `⌃k` › **Go to DDL**, 또는 F1 창에서 다른 키를 주세요 (**Go to DDL** 을 고르고 Enter, 그다음 새 키).
- **`⌥` 키** (`⌥1…9` 워크스페이스, `⌥0` 탐색기, `⌥Enter` 실행, 분할 키) 는 터미널이 Option 을 Meta 로 보내야 합니다: macOS 터미널은 *설정 › 프로파일 › 키보드 › Option 키를 Meta 키로 사용* 을 켜고, iTerm2 는 *Profiles › Keys › Left Option key* 를 *Esc+* 로. tmux 에서는 `~/.tmux.conf` 에 `set -s escape-time 10` 을 넣으면 `Esc` 와 `⌥` 키가 늦거나 두 키로 읽히지 않습니다.
- **`⌃Enter`** 는 kitty 키보드 프로토콜이 필요한데 tmux 와 screen 은 넘겨 주지 않습니다: 그곳에서는 `F5` 나 `⌥Enter` 를 쓰세요.
- **tmux 안에서 24비트 색**: tmux 가 바깥 터미널이 지원한다는 것을 알아야 하고 (tmux 3.2 이상: `set -as terminal-features ",xterm-256color:RGB"`), `COLORTERM=truecolor` 가 tmux 안의 셸까지 와야 합니다. 아니면 Qurry 는 256색 팔레트를 쓰는데, 거의 같아 보입니다.

모든 키는 [TUI 단축키](shortcuts-tui.md) 에 있습니다.
