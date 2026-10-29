# 시작하기

[English](../getting-started.md) | 한국어

Qurry 를 설치하고, 최신으로 유지하고, 빈 창에서 첫 쿼리까지 가는 방법입니다.

## 설치

**TUI + MCP 는 Homebrew 로** (macOS, Linux):

```bash
brew install hkhyyy/qurry/qurry      # 업데이트: brew upgrade qurry
```

또는 [Releases](https://github.com/Hkhyyy/qurry/releases) 에서 최신 빌드를 받습니다:

| | GUI | TUI + MCP |
|---|---|---|
| macOS, Apple Silicon | `Qurry_<version>_aarch64.dmg` | `qurry-cli_<version>_aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `Qurry_<version>_x64.dmg` | `qurry-cli_<version>_x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `Qurry_<version>_amd64.AppImage` 또는 `.deb` | `qurry-cli_<version>_x86_64-unknown-linux-gnu.tar.gz` |

- **macOS**: `Qurry.app` 을 응용 프로그램 폴더로 끌어 넣습니다. 아직 코드 서명이 없어서 처음 열 때 "확인되지 않은 개발자" / "손상됨" 경고가 뜹니다. `xattr -dr com.apple.quarantine /Applications/Qurry.app` 을 실행하거나 앱을 우클릭 → 열기 하세요.
- **Linux**: `chmod +x Qurry_*.AppImage` 후 실행하거나 `sudo apt install ./Qurry_<version>_amd64.deb`. GUI 는 WebKitGTK 4.1 이 필요합니다 (Ubuntu 22.04+, Debian 12+, Fedora 38+).
- **TUI / MCP**: `tar -xzf qurry-cli_….tar.gz && mkdir -p ~/.local/bin && mv qurry-tui qurry-mcp ~/.local/bin/` (macOS 는 먼저 `xattr -d com.apple.quarantine qurry-tui qurry-mcp`).

`main` 이 바뀔 때마다 새 릴리스가 자동으로 나옵니다.

## 업데이트

두 앱 모두 시작할 때와 실행 중 한 시간마다 GitHub 에 새 릴리스가 있는지 확인하고 (GitHub API 요청 한 번), 있으면 알려 줍니다. 바로 확인할 수도 있습니다.

- **GUI** — 상태 표시줄에 `⬆ vX.Y.Z available` 버튼이 나타나고, 지금 버전 이후 바뀐 내용을 버전별로 보여 줍니다. **Update & Restart** 는 새 버전을 받아 서명을 확인하고 앱을 바꾼 뒤 열려 있던 탭 그대로 다시 엽니다. 안 되면 릴리스 페이지에서 직접 받으면 됩니다. **Qurry › Check for Updates…** 또는 **Settings › Appearance › Updates** 에서 바로 확인할 수 있고, 자동 확인도 끌 수 있습니다. 앱 안 업데이트는 0.1.9 (Apple Silicon), Intel / Linux 는 첫 릴리스부터 됩니다.
- **TUI** — 상태 줄에 `⬆ vX.Y.Z available` 이 보입니다. **⌃k › Update Qurry & Restart** 는 릴리스를 받아 서명된 `SHA256SUMS` 로 확인하고 `qurry-tui` · `qurry-mcp` 를 그 자리에서 바꾼 뒤 탭 그대로 다시 시작합니다. **⌃k › Check for Updates** 는 바로 확인하고 바뀐 내용을 보여 줍니다. 셸에서는 `qurry-tui --update`. 소스에서 빌드한 TUI (`target/release`) 는 확인하거나 바꾸지 않으니 `git pull` 과 `cargo build` 로 올리세요. Homebrew 로 설치했다면 `brew upgrade qurry`.
- 확인을 끄려면 `qurry-tui --no-update-check` 또는 `QURRY_NO_UPDATE_CHECK=1`.

## 처음 쓰기

### GUI

1. **Data Source 추가.** Database Explorer 위의 `+` 를 누르거나 `⌘n`, 데이터베이스 종류를 고르고 호스트 · 포트 · 사용자 · 비밀번호를 넣습니다. **Test Connection** 은 서버 버전을 보여 줍니다. SQLite 나 SQL Server 같은 종류는 작은 드라이버가 먼저 필요한데, 창에서 설치하라고 안내합니다 ([드라이버](data-sources.md#드라이버-필요할-때-추가)).
2. **연결하고 둘러보기.** 탐색기에서 Data Source 를 펼칩니다. Data Source (또는 데이터베이스) 우클릭 › **Manage Shown Schemas…** 로 보일 데이터베이스와 스키마를 고릅니다.
3. **쿼리 실행.** Data Source 를 고르고 `⇧⌘L` 로 새 쿼리 콘솔을 엽니다. SQL 을 쓰고 `⌘↩`. 편집기에 문장이 여러 개면 무엇을 실행할지 묻고, 선택한 부분은 그대로 실행합니다. `⌘F2` 는 실행 중인 문장을 취소합니다.
4. **테이블 열고 고치기.** 테이블을 더블클릭 (또는 `⌘↓`). 셀은 그 자리에서 고치고 (`↩`), `⌘n` 으로 행 추가, `⌘⌫` 로 삭제, `⌘↩` 로 모든 변경을 트랜잭션 하나로 저장합니다.
5. **무엇이든 찾기.** `⇧` 를 두 번 (Search Everywhere) 누르면 테이블 · 컬럼 · 열린 탭 · 명령을 찾습니다. `⇧⌘A` 는 모든 명령을 단축키와 함께, `⇧⌘/` 는 전체 키맵을 보여 줍니다.

### TUI

터미널에서 `qurry-tui` 를 실행합니다. (아래 키는 macOS 표기입니다. Linux 에서는 `⌃` 가 `Ctrl`, `⌥` 가 `Alt` 입니다.)

1. **Data Source 추가.** 탐색기에서 `n`, 폼을 채우고 (`Tab` 이동, `Space` / `←→` 선택 바꾸기, `⌃t` 연결 시험) `⌃s` 로 저장합니다. `e` 는 고른 Data Source 를 고칩니다.
2. **연결하고 둘러보기.** Data Source 에서 `Enter` 하면 연결하고 펼칩니다. `s` 로 보일 데이터베이스와 스키마를 고릅니다.
3. **쿼리 실행.** `⌃l` 로 고른 Data Source 의 콘솔을 엽니다. `F5` (또는 `⌃Enter` / `⌥Enter`) 로 실행합니다.
4. **테이블 열고 고치기.** 테이블에서 `Enter` 하면 행이 열립니다. `Enter` / `F2` 로 셀 편집, `a` 행 추가, `d` 행 삭제 표시, `⌃s` 저장.
5. **뭘 눌러야 할지 모르겠으면?** `⌃p` 는 테이블 · 컬럼 · 탭 · 명령을 찾고, `⌃k` 는 모든 명령을 키와 함께 보여 주고, `F1` 은 키맵입니다. 상태 줄에는 지금 보고 있는 곳에서 쓰는 키가 늘 나옵니다.

### Data Source 는 함께 씁니다

GUI · TUI · MCP 서버는 같은 Data Source 와 비밀번호를 읽습니다. 한 곳에서 만든 연결이 나머지에도 나타납니다.

## 설정이 저장되는 곳

- 설정 폴더: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). dbtool 시절 Data Source 와 비밀번호를 그대로 쓰려고 폴더 이름은 예전 그대로입니다.
- 바꾼 단축키는 그 폴더의 `keymap.json` 에 저장됩니다 (`"gui"` · `"tui"` 구역, 바꾼 것만).
- 비밀번호는 그 폴더의 `secrets.json` 에 AES-GCM 으로 암호화해 저장합니다. 폴더는 0700, 파일은 0600 입니다. 기본은 키도 같은 폴더 (`secret.key`) 에 있어서, 파일을 읽을 수 있는 사람 (또는 백업처럼 이 폴더의 복사본을 가진 사람) 은 비밀번호를 풀 수 있습니다. 키를 시스템 키체인 (macOS Keychain, 리눅스 Secret Service, Windows 자격 증명 관리자) 에 두려면: GUI **Settings › Passwords** · TUI `⌃k` › **Password Key…**. 키체인에는 키 하나만 들어갑니다. 앱마다 (GUI, TUI, MCP 서버) 한 번씩 접근을 묻고, macOS 에서는 Qurry 가 코드 서명되기 전까지 업데이트할 때마다 다시 묻습니다. SSH 로 쓰거나 키체인이 없는 헤드리스 리눅스에서는 폴더에 두세요. 키를 옮기기 전에 그 컴퓨터의 Qurry 앱을 모두 업데이트하세요: Qurry 0.1.0, 0.1.1 은 쓸 수 없는 `secret.key` 를 새 키로 바꿔서, 그 뒤 그 버전이 저장한 비밀번호는 다른 앱이 읽지 못합니다.

---

[← 사용설명서](README.md)
