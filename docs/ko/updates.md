# 업데이트

[English](../updates.md) | 한국어

Qurry 가 새 버전을 찾고 설치하는 방법과 stable · beta 채널입니다.

## 업데이트 채널

| 채널 | 받는 것 |
|---|---|
| **Stable** (기본) | 버전 태그 (`vX.Y.Z`) 로 만든 릴리스. 변경이 모이면 메인테이너가 태그를 붙입니다. |
| **Beta** | `main` 에 머지될 때마다 만드는 사전 릴리스 (`vX.Y.Z-beta.N`, 하루 몇 개까지) 도 받습니다. 둘 중 더 새 것을 주므로 `X.Y.Z` 가 나오면 그것도 받습니다. |

채널은 GUI **Settings › Updates › Channel**, TUI **⌃k › Update Channel…** 에서 고릅니다. GUI 와 TUI 가 같은 설정을 씁니다 ([설정 폴더](getting-started.md#설정이-저장되는-곳) 의 `settings.json`). `qurry-tui --update` 도 마찬가지입니다.

beta 는 `main` 에 머지될 때마다, stable 릴리스는 버전 태그로 만들어집니다. 메인테이너가 릴리스를 만들고 서명하는 방법: [릴리스와 서명](release-signing.md).

beta 에서 stable 로 돌아가도 이전 버전을 설치하지는 않습니다: 지금 beta 보다 새 stable 릴리스가 나올 때까지 그 beta 를 씁니다. 바로 beta 를 벗어나려면 [Releases](https://github.com/Hkhyyy/qurry/releases) 에서 최신 stable 릴리스를 직접 설치하세요.

## 확인과 설치

두 앱 모두 시작할 때와 실행 중 한 시간마다 GitHub 에 채널의 새 릴리스가 있는지 확인하고 (GitHub API 요청 한 번), 있으면 알려 줍니다. 바로 확인할 수도 있습니다.

- **GUI** — 상태 표시줄에 `⬆ vX.Y.Z available` 버튼이 나타나고, 지금 버전 이후 바뀐 내용을 버전별로 보여 줍니다. **Update & Restart** 는 새 버전을 받아 서명을 확인하고 앱을 바꾼 뒤 열려 있던 탭 그대로 다시 엽니다. 안 되면 릴리스 페이지에서 직접 받으면 됩니다. **Qurry › Check for Updates…** 또는 **Settings › Updates** 에서 바로 확인할 수 있고, 자동 확인도 끌 수 있습니다. 앱 안 업데이트는 0.1.9 (Apple Silicon), Intel / Linux 는 첫 릴리스부터 됩니다.
- **TUI** — 상태 줄에 `⬆ vX.Y.Z available` 이 보입니다. **⌃k › Update Qurry & Restart** 는 릴리스를 받아 서명된 `SHA256SUMS` 로 확인하고 `qurry-tui` · `qurry-mcp` 를 그 자리에서 바꾼 뒤 탭 그대로 다시 시작합니다. **⌃k › Check for Updates** 는 바로 확인하고 바뀐 내용을 보여 줍니다. 셸에서는 `qurry-tui --update`. 소스에서 빌드한 TUI (`target/release`) 는 확인하거나 바꾸지 않으니 `git pull` 과 `cargo build` 로 올리세요. Homebrew 로 설치했다면 `brew upgrade qurry` 이고, Homebrew 에는 stable 릴리스만 올라갑니다.
- 확인을 끄려면 `qurry-tui --no-update-check` 또는 `QURRY_NO_UPDATE_CHECK=1`.

0.3.0 이전 앱은 채널을 모르고 stable 릴리스만 봅니다. 가장 새 릴리스 30 개 안에서 찾으므로, 릴리스 워크플로는 가장 새 beta 20 개만 남깁니다. 그보다 오래된 beta 의 릴리스 페이지는 지워집니다. 드라이버 플러그인은 앱 자기 버전의 릴리스에서 받으므로, 페이지가 지워진 beta 는 더 이상 드라이버를 설치할 수 없습니다: 그렇다고 알려 주고, Qurry 를 업데이트하면 다시 됩니다.

## macOS 12 이하

GUI 는 0.3.0 부터 macOS 13 Ventura 이상이 필요합니다 (WebAssembly 를 쓰는데, Safari 16 엔진이 있어야 합니다). macOS 12 이하에서는:

- 거기서 도는 마지막 버전인 Qurry 0.2.x 를 계속 쓰세요. 그 앱에서 **Update & Restart** 를 누르지 마세요: 열리지 않는 버전이 설치됩니다.
- 이미 눌러서 macOS 가 "이 버전의 응용 프로그램 Qurry 는 이 버전의 macOS 에서 사용할 수 없습니다" 라고 하면, [Releases](https://github.com/Hkhyyy/qurry/releases) 에서 가장 새 `0.2.x` DMG 를 받아 `Qurry.app` 을 다시 응용 프로그램 폴더로 끌어 넣으세요. Data Source, 비밀번호, 설정은 그대로입니다. 또는 macOS 를 올리고 (시스템 설정 › 일반 › 소프트웨어 업데이트) 새 버전을 쓰세요.
- TUI 와 MCP 서버는 거기서도 돌고 계속 업데이트됩니다.

---

[← 사용설명서](README.md)
