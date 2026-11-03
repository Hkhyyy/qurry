# Qurry 사용설명서

[English](../README.md) | 한국어

Qurry 가 하는 모든 것을 장별로 정리했습니다. 처음이라면 **시작하기** 부터 보세요. 단축키 장은 필요할 때 찾아보는 참고용입니다.

| 장 | 내용 |
|---|---|
| [시작하기](getting-started.md) | 설치, 업데이트, 첫 Data Source 와 쿼리 (GUI, TUI), 설정이 저장되는 곳 |
| [설정](settings.md) | 테마, 편집기 · 그리드 글꼴, 결과 표의 값 정렬 (NULL, 숫자 …), 업데이트, 키맵, 비밀번호 키를 두는 곳, TUI 와 함께 쓰는 설정 |
| [업데이트](updates.md) | stable · beta 업데이트 채널, 앱 안 업데이트, macOS 12, 릴리스가 만들어지는 방식 |
| [Data Source](data-sources.md) | 지원하는 데이터베이스, SSH / SSL 과 연결 옵션, Production 모드, 드라이버 |
| [워크스페이스](workspaces.md) | 탭과 Data Source 를 따로 묶기, 워크스페이스마다 따로인 탐색기 |
| [DB 도구](database-tools.md) | 관리 리포트 (세션, 락, 느린 쿼리, 용량, vacuum, 권한), 다이어그램, 비교 · 마이그레이션, 백업 · 복원, 테이블 · 뷰 · 스키마 만들기 · 고치기, 가져오기 (CSV / JSON / Excel), 테스트 데이터, 개인정보를 가린 관련 행 복사, 차트, 히스토리 · 스니펫, 실행 계획 |
| [벡터 DB 와 AI 검색](vector-and-rag.md) | Qdrant, Chroma, Milvus, pgvector, 뜻으로 테이블 찾기 (RAG) |
| [MCP 서버](mcp.md) | Claude Code 같은 AI 도구가 데이터베이스를 둘러보고 조회하게 하기 (기본 읽기 전용) |
| [MCP 위협 모델](mcp-security.md) | MCP 설정마다 AI 클라이언트가 닿는 것, 읽기 전용을 지키는 방법, 프롬프트 인젝션, Qurry 가 막지 못하는 것 |
| [안전장치](safety.md) | 읽기 전용 Data Source, 트랜잭션, 확인 창, 운영 작업 기록 · 되돌리기 SQL, 릴리스 서명 |
| [GUI 단축키](shortcuts-gui.md) | GUI 단축키 전체 (앱에서도: `⇧⌘/`) |
| [TUI 단축키](shortcuts-tui.md) | TUI 키 전체 (앱에서도: `F1`) |
| [터미널에서 쓰는 TUI](tui.md) | 색 (NO_COLOR, 256색 · 16색 터미널), 글꼴, 작은 창, tmux 와 ⌥ 키 |
| [개발](development.md) | 소스에서 빌드, 코드 구조, 테스트 |
