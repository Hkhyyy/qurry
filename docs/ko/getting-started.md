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

- **macOS**: GUI 는 macOS 13 Ventura 이상이 필요합니다 (macOS 12 이하는 Qurry 0.2.x 를 계속 쓰세요, [업데이트](updates.md#macos-12-이하) 참고. TUI 와 MCP 서버는 돌아갑니다). `Qurry.app` 을 응용 프로그램 폴더로 끌어 넣습니다. 아직 코드 서명이 없어서 처음 열 때 "확인되지 않은 개발자" / "손상됨" 경고가 뜹니다. `xattr -dr com.apple.quarantine /Applications/Qurry.app` 을 실행하거나 앱을 우클릭 → 열기 하세요.
- **Linux**: `chmod +x Qurry_*.AppImage` 후 실행하거나 `sudo apt install ./Qurry_<version>_amd64.deb`. GUI 는 WebKitGTK 4.1 이 필요합니다 (Ubuntu 22.04+, Debian 12+, Fedora 38+).
- **TUI / MCP**: `tar -xzf qurry-cli_….tar.gz && mkdir -p ~/.local/bin && mv qurry-tui qurry-mcp ~/.local/bin/` (macOS 는 먼저 `xattr -d com.apple.quarantine qurry-tui qurry-mcp`).

stable 릴리스는 버전 태그로 나오고, `main` 이 바뀔 때마다 beta 사전 릴리스도 나옵니다.

## 업데이트

두 앱 모두 새 버전이 나오면 알려 주고 그 자리에서 설치합니다: GUI `⬆ vX.Y.Z available` › **Update & Restart**, TUI **⌃k › Update Qurry & Restart** (또는 `qurry-tui --update`). beta 채널로 바꾸지 않으면 stable 릴리스만 받습니다 (GUI **Settings › Appearance › Updates**, TUI **⌃k › Update Channel…**). 채널, 확인 끄기, macOS 12 는 [업데이트](updates.md) 장에 있습니다.

## 처음 쓰기

### GUI

1. **Data Source 추가.** Database Explorer 위의 `+` 를 누르거나 `⌘n`, 데이터베이스 종류를 고르고 호스트 · 포트 · 사용자 · 비밀번호를 넣습니다. **Test Connection** 은 서버 버전을 보여 줍니다. SQLite 나 SQL Server 같은 종류는 작은 드라이버가 먼저 필요한데, 창에서 설치하라고 안내합니다 ([드라이버](data-sources.md#드라이버-필요할-때-추가)).
2. **연결하고 둘러보기.** 탐색기에서 Data Source 를 펼칩니다. Data Source (또는 데이터베이스) 우클릭 › **Manage Shown Schemas…** 로 보일 데이터베이스와 스키마를 고릅니다.
3. **쿼리 실행.** Data Source 를 고르고 `⇧⌘L` 로 새 쿼리 콘솔을 엽니다. SQL 을 쓰고 `⌘↩`. 편집기에 문장이 여러 개면 무엇을 실행할지 묻고, 선택한 부분은 그대로 실행합니다. 캐럿이 서브쿼리 (`(select …)`, CTE 본문, `in (select …)`) 안에 있으면 DataGrip 처럼 문장 전체 다음에 그 서브쿼리도 안쪽 것부터 보여 줍니다: 편집기가 각 항목이 실행할 부분을 정확히 강조하고, 고른 항목은 그 텍스트만 실행하며 다른 실행과 같은 검사 (읽기 전용, Production 확인, 위험한 문장 확인) 를 거칩니다. `in (1, 2)` 같은 괄호 목록이나 이름이 쿼리 키워드로 시작하는 컬럼 (`(table_name = …)`) 은 보여 주지 않습니다. 실행하는 동안 도구 모음의 ▶ 는 같은 자리에서 ■ 가 됩니다: 누르거나 `⌘F2` 로 취소합니다. 결과 표의 ↻ 도 불러오는 동안 (스크롤로 행을 더 불러올 때도) ■ 가 되고, 그 ■ 는 오고 있는 행만 멈추며 이미 보이는 행은 남습니다. 상태 표시줄에는 지금 탭이 하는 일과 걸린 시간 (마우스를 올리면 취소하는 ✕ 가 보입니다), 편집기에 포커스가 있을 때 캐럿 위치 (`Ln 2, Col 11`), 결과 표에 포커스가 있을 때 현재 셀 (`Row 3, Col 5`) 과, 컬럼이 다 들어가지 않으면 보이는 컬럼 (`cols 3–8 of 14`) 이 나옵니다. 실행한 문장마다 거터에 표시가 붙습니다: ✓ 성공, ✗ 실패, 실행 중에는 도는 표시 (실행은 처음 실패한 문장에서 멈추므로 그 뒤 문장에는 표시가 없습니다). 표시에 마우스를 올리면 언제 실행했는지, 걸린 시간과 행 수 (또는 오류) 가 보이고, 누르면 **Output** 에서 그 문장의 줄을 보여 줍니다. 표시는 마지막 실행의 것만 남습니다: 새로 실행하면 시작하면서 이전 실행의 표시를 지우고, 문장을 고치면 그 표시가 사라집니다. 문장이 실패하면 오류가 편집기에도 표시됩니다 (빨간 물결선, 마우스를 올리면 메시지): 서버가 가리키는 곳 (PostgreSQL 의 position, MySQL 의 줄과 `near '…'`), 서버가 말해 주지 않으면 문장 전체. 문장을 고쳐 다시 실행하세요: 고치거나 다시 실행하면 표시가 사라지고, **Output** 에는 오류가 남습니다 (표시에서는 빠지는 PostgreSQL 의 `(position N)` 도 함께). TUI 에서는 그 자리에 밑줄이 그어지고 상태 줄에 위치가 나옵니다 (`Error at line 3, col 8: …`).
4. **테이블 열고 고치기.** 테이블을 더블클릭 (또는 `⌘↓`). 셀은 그 자리에서 고치고 (`↩`), `⌘n` 으로 행 추가, `⌘⌫` 로 삭제, `⌘↩` 로 모든 변경을 트랜잭션 하나로 저장합니다. `⇧↩` 는 그리드 옆에 **Value Editor** 를 엽니다: 현재 셀의 값 전체를 보여 주고 (JSON 은 들여쓰기와 색으로), 그리드가 화살표 키를 그대로 쓰는 동안 선택을 따라갑니다. 클릭하거나 `⇧↩` 를 다시 누르면 고치고, `⌘↩` 로 적용 (다른 셀로 옮겨도 적용), `⎋` 는 그리드로 돌아가고, `⌘w` 나 `⇧⎋` 는 닫습니다. JSON 은 **{ }** 로 들여쓴 모양과 한 줄 사이를 바꾸고, `json` / `jsonb` 컬럼에 올바른 JSON 이 아닌 글을 넣으면 이유와 함께 거부합니다. 콘솔 결과도 테이블 하나에서 읽은 단순한 `SELECT` 이고 그 테이블의 기본 키 (PK) 열이 모두 들어 있으면 똑같이 고칠 수 있습니다 (별칭이나 일부 열만 골라도 됩니다). 조인은 테이블마다 고칩니다: 기본 키 전체가 결과에 있는 테이블의 열을 그 키로 ([안전 › 조인 결과 수정](safety.md)). 조인에서는 행을 추가하거나 삭제할 수 없습니다. `GROUP BY`, `DISTINCT`, `UNION`, 계산한 열, 뷰, 키가 없는 결과는 읽기 전용이고, 흐린 편집 버튼 (조인은 흐린 🔒 열 헤더) 에 이유가 나옵니다 (예: "Read-only: the result doesn't include the primary key of public.users (id)."). Manual 모드이거나 트랜잭션이 열려 있으면 Submit 은 그 트랜잭션 안에서 실행되고, Commit / Roll back 할 때까지 커밋되지 않습니다. 저장하지 않은 수정이 있을 때 콘솔을 다시 실행하면 먼저 묻습니다.
5. **무엇이든 찾기.** `⇧` 를 두 번 (Search Everywhere) 누르면 테이블 · 컬럼 · 열린 탭 · 명령을 찾습니다. `⇧⌘A` 는 모든 명령을 단축키와 함께, `⇧⌘/` 는 전체 키맵을 보여 줍니다.

### TUI

터미널에서 `qurry-tui` 를 실행합니다. (아래 키는 macOS 표기입니다. Linux 에서는 `⌃` 가 `Ctrl`, `⌥` 가 `Alt` 입니다.)

1. **Data Source 추가.** 탐색기에서 `n`, 폼을 채우고 (`Tab` 이동, `Space` / `←→` 선택 바꾸기, `⌃t` 연결 시험) `⌃s` 로 저장합니다. `e` 는 고른 Data Source 를 고칩니다.
2. **연결하고 둘러보기.** Data Source 에서 `Enter` 하면 연결하고 펼칩니다. `s` 로 보일 데이터베이스와 스키마를 고릅니다.
3. **쿼리 실행.** `⌃l` 로 고른 Data Source 의 콘솔을 엽니다. `F5` (또는 `⌃Enter` / `⌥Enter`) 로 실행하거나, 콘솔 제목의 **[▶ Run]** 을 누릅니다. 실행하는 동안 (또는 결과가 행을 더 불러오는 동안) 그 자리는 **[■ Cancel]** 이 되고, `⌃c` 로도 취소합니다. 창보다 넓은 결과는 아래 테두리에 보이는 컬럼을 보여 줍니다: 선의 굵은 부분이 보이는 곳이고 (누르거나 끌어서 옮깁니다), 오른쪽 끝에 `◀ cols 3–8 of 14 ▶` 가 나옵니다. 줄 번호 칸에 실행한 문장마다 첫 줄에 표시가 붙습니다: `✓` 성공, `✗` 실패, `…` 실행 중 (실행은 처음 실패한 문장에서 멈춥니다). 표시를 누르면 **Output** 이 보입니다. 마지막 실행만 표시되고 (새로 실행하면 이전 표시는 지워집니다), 문장의 줄을 고치면 표시가 사라집니다.
4. **테이블 열고 고치기.** 테이블에서 `Enter` 하면 행이 열립니다. `Enter` / `F2` 로 셀 편집, `a` 행 추가, `d` 행 삭제 표시, `⌃s` 저장. `v` 는 현재 셀의 값 전체를 그리드 옆 패널에 보여 주고 (JSON 은 들여쓰기해서) 커서를 따라갑니다. `v` 나 `⌃w` 로 닫습니다. 테이블 하나에서 PK 와 함께 읽은 콘솔 결과도 똑같이 고치고, 조인은 테이블마다 고칩니다 (조인에서는 `a` / `d` 로 행을 추가 · 삭제할 수 없음). 그 밖의 결과나 열에서 편집 키를 누르면 왜 고칠 수 없는지 상태 줄에 나옵니다.
5. **뭘 눌러야 할지 모르겠으면?** `⌃p` 는 테이블 · 컬럼 · 탭 · 명령을 찾고, `⌃k` 는 모든 명령을 키와 함께 보여 주고, `F1` 은 키맵입니다. 상태 줄에는 지금 보고 있는 곳에서 쓰는 키가 늘 나옵니다.

### Data Source 는 함께 씁니다

GUI · TUI · MCP 서버는 같은 Data Source 와 비밀번호를 읽습니다. 한 곳에서 만든 연결이 나머지에도 나타납니다.

## 설정이 저장되는 곳

- 설정 폴더: `~/Library/Application Support/com.hyunkyo.dbtool` (macOS), `~/.config/com.hyunkyo.dbtool` (Linux). dbtool 시절 Data Source 와 비밀번호를 그대로 쓰려고 폴더 이름은 예전 그대로입니다.
- 바꾼 단축키는 그 폴더의 `keymap.json` 에 저장됩니다 (`"gui"` · `"tui"` 구역, 바꾼 것만).
- GUI 와 TUI 가 함께 쓰는 설정 (테마와 사용자 테마, 업데이트 채널, 키워드 대소문자, 결과 표 정렬, 테이블 컬럼 너비) 은 그 폴더의 `settings.json` 에 저장됩니다 ([설정](settings.md)).
- 비밀번호는 그 폴더의 `secrets.json` 에 AES-GCM 으로 암호화해 저장합니다. 폴더는 0700, 파일은 0600 입니다. 기본은 키도 같은 폴더 (`secret.key`) 에 있어서, 파일을 읽을 수 있는 사람 (또는 백업처럼 이 폴더의 복사본을 가진 사람) 은 비밀번호를 풀 수 있습니다. 키를 시스템 키체인 (macOS Keychain, 리눅스 Secret Service, Windows 자격 증명 관리자) 에 두려면: GUI **Settings › Passwords** · TUI `⌃k` › **Password Key…**. 키체인에는 키 하나만 들어갑니다. 앱마다 (GUI, TUI, MCP 서버) 한 번씩 접근을 묻고, macOS 에서는 Qurry 가 코드 서명되기 전까지 업데이트할 때마다 다시 묻습니다. SSH 로 쓰거나 키체인이 없는 헤드리스 리눅스에서는 폴더에 두세요. 키를 옮기기 전에 그 컴퓨터의 Qurry 앱을 모두 업데이트하세요: Qurry 0.1.0, 0.1.1 은 쓸 수 없는 `secret.key` 를 새 키로 바꿔서, 그 뒤 그 버전이 저장한 비밀번호는 다른 앱이 읽지 못합니다.

---

[← 사용설명서](README.md)
