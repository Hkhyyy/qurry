# 워크스페이스

[English](../workspaces.md) | 한국어

탭과 분할 화면을 묶음으로 따로 둡니다 — 하나는 운영 장애 대응용, 하나는 쓰고 있는 리포트용처럼 — 그리고 아무것도 잃지 않고 오갑니다. 다른 워크스페이스에 가 있는 동안에도 콘솔은 세션 · 열린 트랜잭션 · 결과를 그대로 유지합니다.

- **GUI**: 맨 왼쪽의 얇은 막대에 워크스페이스가 나옵니다 (`+` 로 만들고, 우클릭으로 이름 바꾸기 · 지우기). `⌘1`…`⌘9` 로 바꾸고 (탐색기 토글은 `⌘0`), 탭 우클릭 › **Move to Workspace** 로 탭을 옮깁니다.
- **TUI**: `⌥1`…`⌥9` 바꾸기 (터미널은 `⌘` 를 스스로 쓰므로), `⌥n` 새로, `⌥⇧N` 이름 바꾸기 (또는 `⌃k` › Switch / New / Rename / Delete Workspace…, Move Tab to Workspace…). 탐색기에서 `w` 는 고른 Data Source 를 다른 워크스페이스로 옮깁니다. 워크스페이스가 둘 이상이면 위쪽 띠에 보입니다 (눌러서 바꾸기). 탐색기 토글은 `⌥1` 에서 `⌥0` 으로 옮겼습니다.
- **워크스페이스마다 Database Explorer 가 따로 있습니다**: 그 워크스페이스에 넣은 Data Source 가, 펼쳐 둔 그대로 보입니다. 새 워크스페이스는 비어서 시작합니다 — 탐색기의 `+` 로 Data Source 를 넣거나 (새로 만들거나 이미 있는 것), Data Source 우클릭 › Add to Workspace / Remove from This Workspace (TUI: Data Source 폼 (`e` / `n`) 의 **Workspace** 칸으로 옮기거나 `⌃k` › Add Data Source to This Workspace… / Remove Data Source from This Workspace). Data Source 자체 (호스트, 비밀번호) 는 함께 쓰고, 새로 만든 것은 지금 워크스페이스에 들어갑니다.
- 워크스페이스는 탭 · 탐색기와 함께 저장되어 다음에 열 때 그대로 돌아옵니다.

---

[← 사용설명서](README.md)
