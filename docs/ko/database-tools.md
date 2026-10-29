# DB 도구

[English](../database-tools.md) | 한국어

관리 리포트, 다이어그램, 스키마 · 데이터 비교, 마이그레이션, 백업, 파일 가져오기, 테스트 데이터, 관련 행 복사, 차트, 쿼리 히스토리, 실행 계획입니다.

## 데이터베이스 관리 (PostgreSQL, MySQL)

GUI: Data Source 우클릭 › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

| 묶음 | 리포트 |
|---|---|
| 활동 | **Sessions** (누가 누구를 막는지, 쿼리 · 트랜잭션이 얼마나 됐는지, 3초마다 새로 고침) · **Lock Waits** — 우클릭 / `c` 쿼리 취소, `K` 세션 종료 |
| 성능 | **Slow Queries** — 총 시간 기준 상위 100개 (`pg_stat_statements` / `performance_schema`), 콘솔에서 열거나 EXPLAIN |
| 용량 | **Table Sizes** · **Table Bloat** · **Index Bloat** (지운 행이 차지한 공간, 통계로 어림 — PostgreSQL) · **Table Fragmentation** (MySQL) · **Unused Indexes** · **Duplicate Indexes** · **Foreign Keys Without an Index** — DROP / CREATE INDEX, VACUUM FULL, REINDEX, OPTIMIZE TABLE 생성 |
| 유지보수 | **Vacuum & Dead Rows** · **Vacuum Progress** (지금 도는 VACUUM / autovacuum: 단계, 스캔한 비율 — 취소 가능) · **Transaction ID Age** (wraparound) — VACUUM ANALYZE 생성 (PostgreSQL) |
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

## 객체 만들기 · 고치기 (PostgreSQL, MySQL)

DataGrip 처럼: 우클릭 › **New ▸** Table… / View… / Materialized View… / Sequence… / Schema… / Database…, 테이블 › **Modify Table…** (`⌘F6`), **Rename…** (`⇧F6`). TUI: `⌃k` › New Table… / Modify Table… (`⌃F6`) / New View… / New Schema… / New Database… / New Sequence… / Rename… (`⇧F6`).
- 테이블 디자이너는 컬럼 (이름, 타입, NOT NULL, 기본값, 자동 번호 / identity, PK, 설명), 인덱스, 외래 키 (ON DELETE / ON UPDATE) 를 고칩니다. 고치는 동안 실행될 SQL 이 바로 보입니다: 새 테이블은 `CREATE TABLE`, 바꾸면 `ALTER` 문장들. 이름을 바꾼 컬럼은 키 · 인덱스 · 외래 키가 그대로 따라갑니다.
- TUI 에서는 한 줄을 SQL 처럼 씁니다: 컬럼 `email varchar(100) not null default 'x' auto pk comment 'login'`, 인덱스 `users_email_idx (email) unique`, 외래 키 `orders_user_fk (user_id) -> users (id) on delete cascade`. 키: `a` 컬럼 · `i` 인덱스 · `f` 외래 키 · `↩` 고치기 · `d` 지우기 · `p` PK · `n` 이름 · `c` 설명 · `F5` 실행 · `o` 콘솔로 열기.
- PostgreSQL 은 한 트랜잭션으로 실행하고 (한 문장이라도 실패하면 아무것도 안 바뀜), MySQL 은 문장마다 바로 반영됩니다. Production Data Source 는 먼저 묻고, 운영 작업 기록에 남습니다. **Open in Console** 은 실행하는 대신 SQL 을 콘솔에 넣습니다.

## 데이터 가져오기 (CSV, TSV, JSON, Excel)

테이블 우클릭 › Import/Export › Import Data from File… (또는 스키마 › Import Data into New Table…) · TUI: `⌃k` › Import Data from File…. `.csv`, `.tsv`, `.json` (객체 배열), Excel `.xlsx` / `.xls` / `.ods` (첫 시트, 첫 줄이 컬럼 이름) 를 읽습니다. 파일 컬럼을 테이블 컬럼에 짝지으면 전부 트랜잭션 하나로 넣습니다 — 한 행이라도 실패하면 아무것도 들어가지 않습니다. Excel 날짜는 `YYYY-MM-DD [HH:MM:SS]`, 정수는 `.0` 없이, 빈 칸은 NULL 로 들어갑니다.

## 테스트 데이터 만들기 (PostgreSQL, MySQL)

테이블 우클릭 › Import/Export › Generate Test Data… · TUI: 테이블을 고르고 `⌃k` › Generate Test Data…. 테이블 제약을 지나는 그럴듯한 행을 채웁니다:
- 타입 · 길이 · 정밀도, `NOT NULL`, 이미 있는 행과 겹치지 않는 UNIQUE · PK 값
- enum 값과 `CHECK (col IN (…))` 목록
- 외래 키는 가리키는 테이블에 이미 있는 행에서 고릅니다 (부모를 먼저 채우세요)
- 컬럼 이름으로 내용을 짐작합니다 — `email`, `phone`, `name`, `address`, `city`, `url`, `status`, `price`, `created_at` …
- 자동 번호 · identity · 계산 컬럼은 DB 에 맡깁니다

컬럼마다 어떻게 채울지와 예시를 먼저 보여 주고, 최대 100,000 행을 트랜잭션 하나로 넣습니다.

## 관련 행을 다른 Data Source 로 복사 (PostgreSQL, MySQL)

운영 DB 전체를 복사하지 않고 실제 데이터로 디버깅할 때: 테이블 우클릭 › Import/Export › Copy Related Rows To… · TUI: `⌃k` › Copy Related Rows To….
1. WHERE 조건으로 시작 행을 고릅니다 (예: `id = 42`).
2. 그 행이 가리키는 행 (부모) 은 늘 같이 와서 외래 키가 깨지지 않습니다. 그 행을 가리키는 행 (자식 — 예: 고객의 주문, 그 주문의 품목) 은 고른 단계까지 같이 옵니다.
3. 개인정보는 읽을 때 가립니다 (기본으로 켜져 있고, 창에서 체크를 끄거나 TUI 에서 "Copy as is" 를 고르면 그대로 복사): 이메일, 전화번호, 사람 이름, 주소, 주민 · 카드 번호, 비밀번호 · 토큰, 생일, IP 주소 (컬럼 이름으로 판단). md5 로 가려서 같은 값은 늘 같은 값으로 가려집니다. PK · FK 컬럼은 가리지 않습니다.
4. Preview 는 테이블마다 몇 행인지, 어느 컬럼을 가리는지 보여 줍니다. Copy 는 부모부터 대상 Data Source (같은 종류) 에 트랜잭션 하나로 넣고, 이미 있는 행은 건너뜁니다.

대상에 테이블이 미리 있어야 합니다 ([비교와 마이그레이션](#비교와-마이그레이션) 이 DDL 을 만듭니다). 한 번에 최대 50,000 행.

## 차트

GUI: 결과에서 View as (눈 아이콘) › **Chart**: 막대 · 선 · 파이. X 와 Y (여러 개) 를 고르면 같은 X 의 행은 더하고, **count** 는 행 수를 셉니다. 이미 불러온 행을 씁니다. TUI: `⌃k` › Chart Result 가 가로 막대로 그립니다 — 첫 텍스트 컬럼별 첫 수 컬럼의 합 (없으면 행 수).

## 콘솔 값 추천 (PostgreSQL, MySQL)

자동완성이 테이블에 실제로 있는 값도 추천합니다: `status = '` · `status in ('paid', '` · `name like '` 뒤에는 그 컬럼의 값을, `data->>'` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) 뒤에는 json 안의 키를 많이 나온 순으로 행 수와 함께 보여 줍니다. 따옴표를 치면 열립니다 (따옴표 없이는 `⌃Space` — 숫자는 그대로, 글자는 따옴표를 붙여 넣습니다). 앞쪽 5,000 행 (json 키는 2,000 행) 만 3초 제한의 읽기 전용 트랜잭션으로 읽고, 2분 동안은 다시 읽지 않습니다. 바이너리 컬럼과 json 전체 값은 추천하지 않습니다.

## 쿼리 히스토리 · 스니펫

쿼리 히스토리와 스니펫은 설정 폴더에 저장되고 GUI · TUI 가 함께 씁니다 (`PASSWORD '…'` / `IDENTIFIED BY '…'` 의 비밀번호는 가립니다). GUI: `⌥⌘e` (History / Snippets 탭, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## 실행 계획 (PostgreSQL)

노드마다 자기 몫의 시간 (Explain Analyze) 또는 비용을 보여 주고, 볼 만한 것을 표시합니다 — 큰 테이블의 Seq Scan, 10배 이상 틀린 행 수 추정, 대부분의 행을 버리는 필터, 디스크로 넘친 정렬 · 해시. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze 는 문장을 실제로 실행합니다. 데이터를 바꾸는 문장 (UPDATE, DELETE, INSERT …) 은 PostgreSQL · MySQL 에서 트랜잭션 안에서 실행하고 되돌립니다 — 이미 트랜잭션이 열려 있으면 세이브포인트까지만 — 그래서 아무것도 바뀌지 않습니다.

---

[← 사용설명서](README.md)
