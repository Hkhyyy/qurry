# 터미널에서 쓰는 TUI

[English](../tui.md) | 한국어

`qurry-tui` 는 돌고 있는 터미널에 맞춥니다: 색을 몇 개 쓸 수 있는지, 배경이 밝은지, 창이 얼마나 큰지, tmux 나 터미널이 가져가는 키가 무엇인지. 글꼴까지 모두 환경을 따르고, 설정은 터미널이 배경색을 알려 주지 않을 때 고르는 테마 하나뿐입니다.

## URL 로 접속

온콜 때처럼 Data Source 를 먼저 저장하지 않고, `psql` · `mysql` · `redis-cli` 처럼 바로 접속할 수 있습니다:

```sh
qurry-tui postgres://app@db.internal:5432/shop
qurry-tui --read-only mysql://root@127.0.0.1:3306/shop
qurry-tui redis://cache.internal:6379/2
qurry-tui rediss://default@cache.example.com:6380/0          # TLS
qurry-tui --incident postgres://postgres@db.internal/postgres # 바로 Database Admin › Sessions
qurry-tui --ssh bastion postgres://app@10.0.3.12/shop         # SSH 터널을 거쳐서 (~/.ssh/config 의 Host 도 됨)
qurry-tui --env                                               # DATABASE_URL 의 URL
```

URL 은 탐색기 맨 위에 **임시 Data Source** 를 열고 (종류 옆에 `temporary` 표시), 선택해서 접속합니다. 이 `qurry-tui` 프로세스 안에만 있습니다: `connections.json` 에도 비밀번호 파일에도 쓰지 않고, Qurry 앱 · 다른 `qurry-tui` 창 · MCP 서버는 보지 못하며, 그 탭은 다음에 복원되지 않고, `qurry-tui` 를 끝내면 사라집니다. 폼으로 저장할 수도 없습니다 (메시지가 알려 줍니다). 계속 쓰려면 같은 설정으로 Data Source 를 추가하세요 (탐색기에서 `n`). 그 밖에는 저장된 Data Source 와 똑같이 동작합니다 (콘솔, 테이블, Database Admin, 히스토리, Read-only 규칙). 저장된 Data Source 들은 그 아래에 평소처럼 있습니다. URL (또는 `--env`) 없이 실행하면 `qurry-tui` 는 전과 똑같이 시작합니다.

URL 에 없는 부분은 `psql` · `mysql` · `redis-cli` 처럼 환경에서 가져옵니다 (URL 이 먼저, 그다음 환경 변수, 그다음 `~/.pgpass`):

| | 가져오는 곳 |
|---|---|
| PostgreSQL (`postgres://`, `postgresql://`) | `PGHOST`, `PGPORT`, `PGUSER`, `PGDATABASE`, `PGPASSWORD`, `PGSSLMODE`, 그다음 `~/.pgpass` (또는 `PGPASSFILE`). 기본값은 libpq 와 같습니다: `localhost`, `5432`, 내 로그인 이름, 사용자 이름과 같은 데이터베이스. URL 의 `?host=`, `port`, `user`, `dbname`, `sslmode`, `sslrootcert`, `sslcert`, `sslkey` 도 됩니다 |
| MySQL (`mysql://`) | 비밀번호는 `MYSQL_PWD`. 그 밖에는 `localhost`, `3306`, 내 로그인 이름. `?ssl-mode=`, `ssl-ca`, `ssl-cert`, `ssl-key` |
| Redis (`redis://`, `rediss://`) | 비밀번호는 `REDISCLI_AUTH`. 그 밖에는 `127.0.0.1`, `6379`, 데이터베이스 `0`. 데이터베이스는 경로입니다 (`/2`) |

- **비밀번호.** URL 이 아니라 환경 변수나 `~/.pgpass` 에 두세요. 명령줄은 이 컴퓨터의 다른 사용자도 볼 수 있어서 (`ps`), URL 에 비밀번호가 있으면 상태 줄이 경고합니다. `~/.pgpass` 는 `psql` 처럼 다른 사람이 읽을 수 없을 때만 (`chmod 600 ~/.pgpass`) 읽고, 아니면 무시했다는 것과 고치는 법을 상태 줄이 알려 줍니다. 비밀번호는 Data Source 이름, 상태 줄, 오류, 히스토리, Production 로그 어디에도 나오지 않습니다 (`postgres://app:***@…`). 사용자나 비밀번호의 특수 문자는 URL 에서 %-인코딩합니다 (`@` 는 `%40`, `:` 는 `%3A`). IPv6 호스트는 대괄호로 씁니다 (`postgres://app@[::1]/shop`).
- **TLS** 는 저장된 Data Source 와 같은 규칙입니다. `sslmode=require`, `verify-ca`, `verify-full` (MySQL `ssl-mode=REQUIRED`, `VERIFY_CA`, `VERIFY_IDENTITY`) 은 그 모드로 SSL 을 켭니다. 없으면 (또는 `prefer`) 이 컴퓨터의 서버나 `--ssh` 뒤의 서버는 그대로 접속하고, 원격 PostgreSQL · MySQL 은 여전히 TLS 를 맺어야 합니다: `psql` 의 기본값 `prefer` 처럼 평문으로 물러서지 않습니다. `sslmode=disable` (MySQL `ssl-mode=DISABLED`) 은 이 접속 하나에 대해 직접 고른 "Allow unencrypted connection" 이고, 그때 상태 줄이 `Not encrypted` 라고 알려 줍니다. `rediss://` 는 항상 TLS 를 쓰고 인증서를 확인합니다 (`?sslmode=require` 는 암호화만). TLS 설정이 붙은 `redis://` 는 `rediss://` 를 쓰라는 안내와 함께 거부합니다.
- **`--read-only`** 는 Read-only Data Source 와 똑같이 데이터를 바꾸는 문장을 거부합니다 (PostgreSQL 은 서버의 세션도 읽기 전용입니다). 상태 표시줄에 `READ-ONLY` 가 보입니다.
- **`--incident`** 는 그 Data Source 의 Database Admin › Sessions 를 엽니다 (PostgreSQL, MySQL). 3초마다 새로 읽고, `c` 는 쿼리 취소, `K` 는 세션 종료입니다.
- **`--ssh DEST`** 는 SSH 터널을 거칩니다. `DEST` 는 `[user@]host[:port]` 또는 `~/.ssh/config` 의 `Host` 이고, 그 `HostName`, `User`, `Port`, `IdentityFile`, `IdentityAgent`, `ProxyJump` 가 적용됩니다. ssh-agent 로, 그다음 `IdentityFile` 키 (없으면 `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`) 로 로그인합니다. URL 의 데이터베이스 호스트는 SSH 서버에서 본 주소입니다. 처음 보는 호스트 키는 저장된 Data Source 처럼 지문을 보여 주며 멈춥니다. 확인한 뒤 `⌃k › Trust SSH Host Key` 를 하고 Data Source 를 다시 펼치세요. `~/.ssh/config` 에서 무엇을 읽는지는 [SSH 터널](data-sources.md#지원하는-데이터베이스) 을 보세요.
- **잘못 쓰면** 화면을 열기 전에 무엇이 잘못됐는지 알려 주고 종료 코드 2 로 멈춥니다: 모르는 스킴 (`mongodb://` 는 안 됩니다), 모르는 URL 파라미터, URL 없는 `--read-only`, 잘못 쓴 옵션 (`--readonly` 는 거부하므로 실수로 쓰기 가능한 접속을 열지 않습니다). `qurry-tui --help` 가 쓸 수 있는 형태를 보여 줍니다.

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
