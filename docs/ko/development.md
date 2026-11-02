# 개발

[English](../development.md) | 한국어

소스에서 빌드하기, 코드 구조, 테스트입니다.

## 준비물

Rust (stable), Node.js 22, pnpm 10. GUI 는 WebAssembly 로 컴파일한 Rust (`crates/shared`) 도 쓰므로 이것도 필요합니다:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # crates/shared-wasm/Cargo.toml 의 wasm-bindgen 버전과 같아야 함
```

`pnpm wasm` 이 `src/wasm/pkg` 로 빌드합니다 (커밋하지 않음). `pnpm dev`, `pnpm build`, `pnpm test`, Storybook 스크립트가 먼저 이걸 돌리고, 빠진 게 있으면 무엇을 설치할지 알려 줍니다. [binaryen](https://github.com/WebAssembly/binaryen) 의 `wasm-opt` 가 깔려 있으면 모듈을 더 줄입니다 (로컬에서는 선택. CI 와 릴리스 빌드는 `npm install -g binaryen@132.0.0` 으로 설치하며, 로컬에서도 같은 명령으로 깔 수 있습니다).

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

GUI 는 `src/ui` 의 공통 컴포넌트 (버튼, 도구 막대, 대화상자, 확인 창, 폼 칸, 탭, 안내) 와 디자인 토큰 (간격, 글자 크기, 테마별 색) 으로 만든다. 규칙은 [src/ui/README.md](../../src/ui/README.md). TUI 는 `crates/tui/src/kit` 에서 같은 이름 · 같은 색을 쓴다. 둘 다 원자 · 분자 · 유기체로 나눈다.

```bash
pnpm storybook        # http://localhost:6006 — GUI 컴포넌트를 테마별로, TUI 화면까지
pnpm tui-snapshots    # TUI 를 바꾼 뒤 Storybook 의 TUI 화면을 다시 그린다
```

## 문서

사용설명서는 영어 (`docs/`) 와 한국어 (`docs/ko/`) 가 장마다 짝입니다. 기능이 바뀌면 둘 다 고칩니다. 단축키를 바꾼 뒤에는 `python3 scripts/gen-keymap-docs.py` 로 두 언어의 [shortcuts-gui.md](shortcuts-gui.md) · [shortcuts-tui.md](shortcuts-tui.md) 표를 다시 만듭니다. 두 앱의 바꿀 수 있는 명령 (id · 이름 · 기본 키) 은 표 하나, `crates/shared/src/keymap-table.json` 에 있습니다. id 는 keymap.json 에 저장되는 이름이라 바꾸지 않습니다.

---

[← 사용설명서](README.md)
