# DB 도구

[English](../database-tools.md) | 한국어

관리 리포트, 다이어그램, 스키마 · 데이터 비교, 마이그레이션, 백업, 쿼리 히스토리, 실행 계획입니다.

## 데이터베이스 관리 (PostgreSQL, MySQL)

GUI: Data Source 우클릭 › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

| 묶음 | 리포트 |
|---|---|
| 활동 | **Sessions** (누가 누구를 막는지, 쿼리 · 트랜잭션이 얼마나 됐는지, 3초마다 새로 고침) · **Lock Waits** — 우클릭 / `c` 쿼리 취소, `K` 세션 종료 |
| 성능 | **Slow Queries** — 총 시간 기준 상위 100개 (`pg_stat_statements` / `performance_schema`), 콘솔에서 열거나 EXPLAIN |
| 용량 | **Table Sizes** · **Unused Indexes** · **Duplicate Indexes** · **Foreign Keys Without an Index** — DROP / CREATE INDEX 생성 |
| 유지보수 | **Vacuum & Dead Rows** · **Transaction ID Age** (wraparound) — VACUUM ANALYZE 생성 (PostgreSQL) |
| 서버 | **Server Overview** (가동 시간, 연결 수, 캐시 적중률) · **Replication** 지연 · **Settings** |
| 보안 | **Users & Roles** · **Table Privileges** — GRANT / REVOKE 생성 |

만든 SQL 은 새 콘솔에 열리고, 실행하기 전까지는 아무것도 바뀌지 않습니다. 세션 종료는 먼저 묻고 (pid 와 쿼리를 보여 줌), 읽기 전용 Data Source 에서는 막힙니다.

## 다이어그램 (ERD)

GUI — 스키마나 데이터베이스 우클릭 › Diagrams › Show Diagram: 키와 외래 키가 있는 테이블을 자동 배치 (부모가 위, 자식이 아래). 테이블 끌기, 확대 · 축소, 더블클릭으로 테이블 열기, **Copy as Mermaid**, **Save SVG**. TUI — `⌃k` › Show Diagram: 테이블마다 들어오고 나가는 외래 키를 글로, 그리고 Mermaid 소스 (`y` 로 전체 복사). Data Source 에서 열면 어느 데이터베이스를 그릴지 먼저 묻습니다.

## 비교와 마이그레이션

GUI — Data Source 우클릭 › Tools › Compare & Migrate · TUI — `⌃k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *스키마 비교*: 대상 스키마를 원본처럼 만드는 DDL (테이블, 컬럼, 키, 인덱스, 외래 키, 뷰). DROP 은 맨 뒤에 오고 체크가 꺼져 있습니다. 콘솔에서 열거나 다음 마이그레이션 파일로 저장합니다.
- *데이터 비교*: 기본 키로 맞춘 행 → 대상을 맞추는 INSERT / UPDATE / DELETE.
- *마이그레이션*: `V1__create.sql`, `V2__add_index.sql` … 이 든 폴더를 버전 순서대로 파일마다 트랜잭션으로 적용합니다. 적용한 것은 `qurry_schema_history` 에 기록되고, 적용한 뒤 바뀐 파일은 거부합니다.

## 백업 · 복원

데이터베이스 우클릭 › Tools › Back Up Database… / Restore Database… (TUI: `⌃k`). 이 컴퓨터의 `pg_dump` / `pg_restore` / `psql` 또는 `mysqldump` / `mysql` 을 실행합니다 (Homebrew `libpq`, `mysql-client`, Postgres.app, 리눅스의 흔한 위치도 찾습니다. PostgreSQL 은 서버 버전에 가장 가까운 클라이언트를 씁니다). 비밀번호는 환경 변수로 넘기고, SSH 터널을 쓰고, 진행 상황은 상태 표시줄에 나오며 취소할 수 있습니다.

## 쿼리 히스토리 · 스니펫

쿼리 히스토리와 스니펫은 설정 폴더에 저장되고 GUI · TUI 가 함께 씁니다 (`PASSWORD '…'` / `IDENTIFIED BY '…'` 의 비밀번호는 가립니다). GUI: `⌥⌘e` (History / Snippets 탭, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## 실행 계획 (PostgreSQL)

노드마다 자기 몫의 시간 (Explain Analyze) 또는 비용을 보여 주고, 볼 만한 것을 표시합니다 — 큰 테이블의 Seq Scan, 10배 이상 틀린 행 수 추정, 대부분의 행을 버리는 필터, 디스크로 넘친 정렬 · 해시. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze 는 문장을 실제로 실행합니다. 데이터를 바꾸는 문장 (UPDATE, DELETE, INSERT …) 은 PostgreSQL · MySQL 에서 트랜잭션 안에서 실행하고 되돌립니다 — 이미 트랜잭션이 열려 있으면 세이브포인트까지만 — 그래서 아무것도 바뀌지 않습니다.

---

[← 사용설명서](README.md)
