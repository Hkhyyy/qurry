# 개발

[English](../development.md) | 한국어

소스에서 빌드하기, 코드 구조, 테스트입니다.

## 준비물

Rust, Node.js, pnpm, WebAssembly 도구: [CONTRIBUTING › Setup](../../CONTRIBUTING.md#setup) (영어).

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
crates/shared qurry-shared: GUI 와 함께 쓰는 순수 Rust 로직 (I/O 없음, wasm32 로 빌드됨)
crates/shared-wasm  qurry-shared-wasm: GUI 가 src/lib/shared.ts 로 부르는 wasm-bindgen 래퍼
drivers/*     드라이버 프로그램 (SQLite, DuckDB, SQL Server, ClickHouse, MongoDB, Oracle, Qdrant, Chroma, Milvus)
```

데이터 소스와 저장된 비밀번호는 GUI · TUI · MCP 서버가 (나란히 도는 옛 버전 앱도) 함께 쓰므로, 설정 파일마다 `crates/core` 안에 집이 하나씩 있습니다:

- `store.rs` 가 `connections.json` 과 `connection-access.json` 을 읽고 씀
- `secrets.rs` 가 `secrets.json` 과 비밀번호 키 (`SecretFile`) 를 읽고 씀
- `lib.rs` 의 `open_state` 가 앱 상태를 엶. 모든 앱은 `Store` 와 `SecretFile` 을 직접 만들지 않고 이 함수를 부름

다른 코드가 (테스트 코드는 빼고) 이 파일들을 쓰거나 `SecretFile` 을 만들면 `crates/core/tests/boundaries.rs` 가 실패합니다.

## 테스트

단위 테스트와 실제 PostgreSQL · MySQL · Redis 컨테이너로 도는 테스트를 실행하는 방법: [CONTRIBUTING › Tests](../../CONTRIBUTING.md#tests) (영어). 모든 풀 리퀘스트에서 CI 가 돕니다: Rust 단위 테스트, PostgreSQL (pgvector 포함) / MySQL / Redis / ClickHouse / MongoDB / Qdrant / Chroma / Milvus 컨테이너로 통합 테스트, 같은 테스트를 옛 서버 (PostgreSQL 12, MySQL 5.7, ClickHouse 22.8, MongoDB 5, SQL Server 2017) 로, vitest 와 웹 빌드.

Redis TLS (16391), Sentinel (26392 → 16392), Cluster (17001-17003) 서버는 `crates/core/src/redisdb.rs` 의 테스트 주석에 있습니다 (TLS 인증서 폴더: `QURRY_TEST_TLS_DIR`, 기본 `/tmp/dbtool-tls`).

드라이버 · 벡터 DB 테스트는 서버를 `host:port:user:password:database` 로 주면 (그리고 드라이버를 빌드했으면 — `cargo build -p qurry-driver-<name>`) 돕니다: `QURRY_TEST_CH`, `QURRY_TEST_MSSQL`, `QURRY_TEST_MONGO`, `QURRY_TEST_ORACLE`, `QURRY_TEST_QDRANT`, `QURRY_TEST_CHROMA`, `QURRY_TEST_MILVUS` (예: `QURRY_TEST_QDRANT=127.0.0.1:6333:::qdrant`). pgvector 와 RAG 테스트는 서버에 `vector` 확장이 있으면 (`pgvector/pgvector:pg16`) `QURRY_TEST_PG` 를 씁니다. RAG 테스트는 가짜 임베딩 서버를 써서 모델이 필요 없습니다.

TUI 테스트는 DB 없이 앱을 움직일 수 있습니다: `crates/tui/src/testkit.rs` 가 정해진 행으로 테이블 탭을 열고, 키를 보내고 (`h.key("Ctrl+A")`, `h.type_str("ban")`), 화면을 글로 그립니다 (`h.screen(120, 30)`). `crates/tui/src/tests/` 와 `crates/tui/src/input/` 안 키 처리 옆의 테스트를 보세요.

## UI 컴포넌트와 Storybook

GUI 는 `src/ui` 의 공통 컴포넌트 (버튼, 도구 막대, 대화상자, 확인 창, 폼 칸, 탭, 안내) 와 디자인 토큰 (간격, 글자 크기, 테마별 색) 으로 만든다. 규칙은 [src/ui/README.md](../../src/ui/README.md). TUI 는 `crates/tui/src/kit` 에서 같은 이름 · 같은 색을 쓴다. 둘 다 원자 · 분자 · 유기체로 나눈다. Storybook 실행: [CONTRIBUTING › UI and Storybook](../../CONTRIBUTING.md#ui-and-storybook) (영어).

## 문서

사용설명서는 영어 (`docs/`) 와 한국어 (`docs/ko/`) 가 장마다 짝입니다. 기능이 바뀌면 둘 다 고칩니다. 단축키를 바꾼 뒤에는 `python3 scripts/gen-keymap-docs.py` 로 두 언어의 [shortcuts-gui.md](shortcuts-gui.md) · [shortcuts-tui.md](shortcuts-tui.md) 표를 다시 만듭니다. 두 앱의 바꿀 수 있는 명령 (id · 이름 · 기본 키) 은 표 하나, `crates/shared/src/keymap-table.json` 에 있습니다. id 는 keymap.json 에 저장되는 이름이라 바꾸지 않습니다.

---

[← 사용설명서](README.md)
