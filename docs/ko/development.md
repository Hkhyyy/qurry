# 개발

[English](../development.md) | 한국어

소스에서 빌드하기, 코드 구조, 테스트입니다.

## 빌드와 실행

```bash
# GUI (개발 모드)
pnpm install
pnpm tauri dev

# GUI 설치 파일 (macOS .dmg, Linux .AppImage / .deb)
pnpm tauri build

# TUI / MCP
cargo build --release -p qurry-tui -p qurry-mcp
ln -sf "$PWD/target/release/qurry-tui" ~/.local/bin/qurry-tui
ln -sf "$PWD/target/release/qurry-mcp" ~/.local/bin/qurry-mcp

qurry-tui            # F1 로 키맵
qurry-tui --help     # 키맵 출력
```

## 코드 구조

```
crates/core   qurry-core: 드라이버 (PostgreSQL, MySQL, Redis), 세션, SSH 터널, 저장소, AppState (모든 기능)
src-tauri     GUI 백엔드: Tauri 명령이 AppState 메서드를 바로 부름
src           GUI 프런트엔드 (React + Monaco)
crates/tui    TUI (ratatui)
crates/mcp    MCP 서버 (stdio JSON-RPC)
crates/driver qurry-driver: 드라이버 프로토콜 (stdio 로 한 줄에 JSON 요청 / 응답 하나) + REST 도우미
drivers/*     드라이버 프로그램 (SQLite, DuckDB, SQL Server, ClickHouse, MongoDB, Oracle, Qdrant, Chroma, Milvus)
```

## 테스트

실패하는 테스트를 먼저 씁니다: 버그 수정은 그 버그를 재현하는 테스트로, 기능은 새 동작을 시험하는 테스트로 시작합니다 (테스트할 수 없는 화면 · 터미널 키는 손으로 확인). 모든 풀 리퀘스트에서 CI 가 돕니다: Rust 단위 테스트, PostgreSQL (pgvector 포함) / MySQL / Redis / ClickHouse / MongoDB / Qdrant / Chroma / Milvus 컨테이너로 통합 테스트, 같은 테스트를 옛 서버 (PostgreSQL 12, MySQL 5.7, ClickHouse 22.8, MongoDB 5, SQL Server 2017) 로, vitest 와 웹 빌드.

```bash
cargo test -p qurry-core -p qurry-tui -p qurry-mcp   # 단위 테스트 (DB 없이)
pnpm test                                               # GUI 로직 (src/lib/*.test.ts, vitest)
cargo test --workspace -- --include-ignored --skip tunnel   # 로컬 PostgreSQL 과 아래 컨테이너 필요
QURRY_TEST_PG=127.0.0.1:15499:postgres:shop cargo test -p qurry-core session -- --ignored   # 다른 곳의 PostgreSQL (host:port:user:db)
```

통합 테스트용 컨테이너:

```bash
docker run -d --name dbtool-mysql-test -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop -p 13306:3306 mysql:8.4
docker run -d --name dbtool-redis-test -p 16379:6379 redis:7-alpine
# TLS (16391), Sentinel (26392 → 16392), Cluster (17001-17003) 서버: crates/core/src/redisdb.rs 의 테스트 주석 참고
# (TLS 인증서 폴더: QURRY_TEST_TLS_DIR, 기본 /tmp/dbtool-tls)
```

드라이버 · 벡터 DB 테스트는 서버를 `host:port:user:password:database` 로 주면 (그리고 드라이버를 빌드했으면 — `cargo build -p qurry-driver-<name>`) 돕니다: `QURRY_TEST_CH`, `QURRY_TEST_MSSQL`, `QURRY_TEST_MONGO`, `QURRY_TEST_ORACLE`, `QURRY_TEST_QDRANT`, `QURRY_TEST_CHROMA`, `QURRY_TEST_MILVUS` (예: `QURRY_TEST_QDRANT=127.0.0.1:6333:::qdrant`). pgvector 와 RAG 테스트는 서버에 `vector` 확장이 있으면 (`pgvector/pgvector:pg16`) `QURRY_TEST_PG` 를 씁니다. RAG 테스트는 가짜 임베딩 서버를 써서 모델이 필요 없습니다.

TUI 테스트는 DB 없이 앱을 움직일 수 있습니다: `crates/tui/src/testkit.rs` 가 정해진 행으로 테이블 탭을 열고, 키를 보내고 (`h.key("Ctrl+A")`, `h.type_str("ban")`), 화면을 글로 그립니다 (`h.screen(120, 30)`). `crates/tui/src/input.rs` 끝의 테스트를 보세요.

## UI 컴포넌트와 Storybook

GUI 는 `src/ui` 의 공통 컴포넌트 (버튼, 도구 막대, 대화상자, 확인 창, 폼 칸, 탭, 안내) 와 디자인 토큰 (간격, 글자 크기, 테마별 색) 으로 만든다. 규칙은 [src/ui/README.md](../../src/ui/README.md). TUI 는 `crates/tui/src/kit.rs` 에서 같은 이름 · 같은 색을 쓴다.

```bash
pnpm storybook        # http://localhost:6006 — GUI 컴포넌트를 테마별로, TUI 화면까지
pnpm tui-snapshots    # TUI 를 바꾼 뒤 Storybook 의 TUI 화면을 다시 그린다
```

## 문서

사용설명서는 영어 (`docs/`) 와 한국어 (`docs/ko/`) 가 장마다 짝입니다. 기능이 바뀌면 둘 다 고칩니다. 단축키를 바꾼 뒤에는 `python3 scripts/gen-keymap-docs.py` 로 두 언어의 [shortcuts-gui.md](shortcuts-gui.md) · [shortcuts-tui.md](shortcuts-tui.md) 표를 다시 만듭니다.

---

[← 사용설명서](README.md)
