# DB 도구

[English](../database-tools.md) | 한국어

관리 리포트, 테이블 구조, 다이어그램, 스키마 · 데이터 비교, 마이그레이션, 백업, 파일 가져오기, 테스트 데이터, 관련 행 복사, 차트, 쿼리 히스토리, 실행 계획입니다.

## 데이터베이스 관리 (PostgreSQL, MySQL, MariaDB)

GUI: Data Source 우클릭 › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

리포트는 데이터베이스 하나에서 돌고, 그 데이터베이스가 리포트 선택 옆에 보입니다 (TUI: 그리드 제목). 데이터베이스에서 열면 그 데이터베이스, Data Source 에서 열면 그 Data Source 의 기본 데이터베이스입니다 (비어 있으면 `postgres`). 거기서 다른 것을 고르면 (TUI: `d`) 그 데이터베이스에서 리포트를 다시 돌리고, 자동 새로 고침도 고른 데이터베이스에서 계속됩니다. 목록은 콘솔의 데이터베이스 선택과 같이 서버의 모든 데이터베이스를 보여 주고, 지금 데이터베이스가 맨 앞입니다 (Manage Shown Schemas 는 탐색기만 거릅니다). MySQL 은 스키마를 보여 주고, 기본 스키마가 없는 Data Source 는 *(all schemas)* 도 있습니다. 데이터베이스 하나로 고정한 Data Source 는 그 하나만 보여 주고 바꿀 수 없습니다. Sessions, Lock Waits, Users & Roles 는 어느 데이터베이스를 골라도 서버 전체를 보여 줍니다. 서버가 고른 데이터베이스를 거절하면 (없거나, 계정에 CONNECT 권한이 없음) 리포트가 그렇다고 알려 주니 다른 것을 고르세요. 목록은 리포트의 데이터베이스를 통해 읽고, 안 되면 Data Source 의 기본 데이터베이스를 통해 읽습니다. 그래서 연결할 수 없는 데이터베이스를 골라도 목록은 남습니다. 둘 다 안 되면 (Data Source 에서 열었는데 계정이 기본 데이터베이스에 연결할 수 없을 때) 리포트 위에 (TUI: 상태 줄) 그렇다고 알려 줍니다: Data Source Properties › General › Database (TUI: Data Source 양식의 Database 칸) 에 연결할 수 있는 기본 데이터베이스를 넣으세요.

| 묶음 | 리포트 |
|---|---|
| 활동 | **Sessions** (누가 누구를 막는지, 쿼리 · 트랜잭션이 얼마나 됐는지, 3초마다 새로 고침) · **Lock Waits** — 우클릭 / `c` 쿼리 취소, `K` 세션 종료 |
| 성능 | **Slow Queries** — 총 시간 기준 상위 100개 (`pg_stat_statements` / `performance_schema`), 콘솔에서 열거나 EXPLAIN (값 자리에 `$1` / `?` / `(...)` 가 들어간 문장이라 PostgreSQL 16+ 는 `EXPLAIN (GENERIC_PLAN)`, 그 밖에는 값을 먼저 채워야 함). `pg_stat_statements` 가 없으면 무엇이 빠졌는지 알려 줌: 라이브러리가 이미 미리 로드된 서버(RDS / Aurora 는 보통 그렇다)는 `CREATE EXTENSION pg_stat_statements;` 만 (superuser — RDS / Aurora 는 `rds_superuser`, Cloud SQL 은 `cloudsqlsuperuser`), 아니면 먼저 `shared_preload_libraries` 에 넣고 재시작 (RDS / Aurora / Cloud SQL 은 파라미터 그룹 / 데이터베이스 플래그를 바꾼 뒤 재부팅), 확장이 아직 없으면 그다음 생성 |
| 용량 | **Table Sizes** · **Table Bloat** · **Index Bloat** (지운 행이 차지한 공간, 통계로 어림 — PostgreSQL) · **Table Fragmentation** (MySQL) · **Unused Indexes** (언제부터 센 스캔 수인지 함께) · **Duplicate Indexes** · **Foreign Keys Without an Index** — DROP / CREATE INDEX, VACUUM FULL, REINDEX, OPTIMIZE TABLE 생성 |
| 유지보수 | **Vacuum & Dead Rows** · **Vacuum Progress** (지금 도는 VACUUM, VACUUM FULL / autovacuum: 단계, 스캔한 비율 — 취소 가능) · **Transaction ID Age** (wraparound) — VACUUM ANALYZE 생성 (PostgreSQL) |
| 서버 | **Server Overview** (가동 시간, 연결 수, 캐시 적중률) · **Replication** 지연 · **Settings** |
| 보안 | **Users & Roles** · **Table Privileges** — GRANT / REVOKE 생성 |

만든 SQL 은 새 콘솔에 열리고, 실행하기 전까지는 아무것도 바뀌지 않습니다. 세션 종료는 먼저 묻고 (pid 와 쿼리를 보여 줌), 읽기 전용 Data Source 에서는 막힙니다. 다른 사용자의 세션을 KILL 할 수 없는 Amazon RDS / Aurora MySQL 에서는 `mysql.rds_kill` / `mysql.rds_kill_query` 를 씁니다.

리포트 쿼리는 30초가 지나면 멈춥니다 (`statement_timeout` / `max_execution_time` 은 그 쿼리에만 걸리고 연결에는 남지 않음). 서버 · 역할 · 연결에 더 짧은 제한이 걸려 있으면 그것을 따릅니다. 제한에 걸린 리포트는 그렇다고 알려 줍니다. 트랜잭션을 허용하지 않는 statement pooling 모드의 pgbouncer 를 거치면 PostgreSQL 리포트는 제한 없이 돕니다. Sessions · Lock Waits 는 앞선 새로 고침이 아직 돌고 있으면 그번은 건너뜁니다.

## 다이어그램 (ERD)

GUI — 스키마나 데이터베이스 우클릭 › Diagrams › Show Diagram: 키와 외래 키가 있는 테이블을 자동 배치 (부모가 위, 자식이 아래). 테이블 끌기, 확대 · 축소, 더블클릭으로 테이블 열기, **Copy as Mermaid**, **Save SVG**. TUI — `⌃k` › Show Diagram: 테이블마다 들어오고 나가는 외래 키를 글로, 그리고 Mermaid 소스 (`y` 로 전체 복사). Data Source 에서 열면 어느 데이터베이스를 그릴지 먼저 묻습니다.

## 비교와 마이그레이션

GUI — Data Source 우클릭 › Tools › Compare & Migrate · TUI — `⌃k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *스키마 비교*: 대상 스키마를 원본처럼 만드는 DDL (테이블, 컬럼, 키, 인덱스, 외래 키, 뷰). DROP 은 맨 뒤에 오고 체크가 꺼져 있습니다. 콘솔에서 열거나 다음 마이그레이션 파일로 저장합니다.
- *데이터 비교*: 기본 키로 맞춘 행 → 대상을 맞추는 INSERT / UPDATE / DELETE.
- *마이그레이션*: `V1__create.sql`, `V2__add_index.sql` … 이 든 폴더를 버전 순서대로 파일마다 트랜잭션으로 적용합니다. 적용한 것은 `qurry_schema_history` 에 기록되고, 적용한 뒤 바뀐 파일은 거부합니다.

## 백업 · 복원

데이터베이스 우클릭 › Tools › Back Up Database… / Restore Database… (TUI: `⌃k`). 이 컴퓨터의 `pg_dump` / `pg_restore` / `psql` 또는 `mysqldump` / `mysql` 을 실행합니다 (Homebrew `libpq`, `mysql-client`, `mariadb`, Postgres.app, 리눅스의 흔한 위치와 MariaDB 자체 이름인 `mariadb-dump` / `mariadb` 도 찾습니다. 덤프 도구는 서버 버전에 가장 가까운 것을 씁니다 — MySQL / MariaDB 는 같은 종류 중에서 고르고, 더 예전이거나 다른 종류의 클라이언트밖에 없어서 실패하면 무엇을 설치할지 알려 줍니다. MySQL 복원은 서버와 같은 종류의 `mysql` 중 셸 명령을 끌 수 있는 (`--system-command=OFF`, MariaDB 는 `--sandbox`) 가장 새 버전을 씁니다). SSH 터널을 쓰고, 진행 상황은 상태 표시줄에 나오며 취소할 수 있습니다. 백업 파일과 임시 덤프는 본인만 읽을 수 있고, 비밀번호는 그 서버에만 맞는 개인 파일로 도구에 넘깁니다. `.sql` 파일을 복원할 때는 SQL 만 실행합니다: 파일 안의 psql · mysql 클라이언트 명령 (프로그램 실행, 파일 쓰기, 다른 곳에 접속) 은 꺼 둡니다 — psql 17.6 / 16.10 / 15.14 / 14.19 / 13.22 이상이 필요해서, 그보다 예전 psql 로는 `.sql` 파일을 복원하지 않습니다 (커스텀 형식 백업은 `pg_restore` 로 복원하므로 상관없습니다 — 다만 가장 가까운 `pg_restore` 가 17 이상이고 서버가 그보다 예전이면, 서버가 모르는 설정을 보내므로 psql 로 대신 복원합니다). `\connect` 로 데이터베이스를 바꾸는 덤프 (`pg_dump -C`, `pg_dumpall`) 는 안내와 함께 멈추니, psql 로 직접 복원하세요.

MySQL 백업에는 `SET @@GLOBAL.GTID_PURGED` 를 넣지 않아서 (`--set-gtid-purged=OFF`, mysqldump 5.6.9 이상) GTID 서버 (RDS 등) 의 덤프를 다른 서버에 복원할 수 있고, 8.0 이상 `mysqldump` 로 5.7 · MariaDB 서버를 백업할 때는 컬럼 통계를 건너뜁니다 (`--column-statistics=0`). 백업 대화상자의 **Leave out DEFINER= clauses** 를 켜면 (TUI 는 파일 경로 다음 질문에 y) 루틴 · 뷰 · 트리거 · 이벤트를 `DEFINER=` 사용자 없이 써서, SUPER 가 없는 관리형 서비스에도 복원됩니다 (복원한 사용자의 것이 됩니다). **Data only** 에도 적용됩니다: 데이터만 받은 덤프에도 루틴 · 트리거 · 이벤트는 들어갑니다 (뷰는 빠집니다). MySQL 복원은 원자적이지 않습니다: 문장을 하나씩 실행하므로 하나가 실패해도 그 전에 실행된 것은 남습니다 (PostgreSQL 복원은 한 트랜잭션입니다).

## 테이블 구조 (DDL 탭)

테이블에서 **Go to DDL** (`⌘b`, TUI `⌃b`) 을 하면 SQL 이 열립니다. 탭의 **SQL | Structure** 전환 (포커스가 있으면 `← →`, TUI: `v`) 은 같은 테이블을 읽기 전용 표로 보여 줍니다: 컬럼 (이름, 타입, NULL 허용, 기본값, 설명, PK 표시), 키와 제약, 인덱스와 그 컬럼, 양쪽 방향의 외래 키 (이 테이블이 참조하는 것과 이 테이블을 참조하는 테이블, ON UPDATE / ON DELETE 포함), 트리거, 테이블 옵션과 설명 (PostgreSQL: owner, tablespace, storage parameter, partition key, parent; MySQL: engine, charset, collation, row format). 새 DDL 탭은 마지막에 쓴 보기로 열리고 (기본은 SQL), 그 뒤로는 탭마다 자기 보기를 유지합니다. 툴바의 **Reload** 아이콘 (TUI `r`) 은 탭이 보여 주는 것을 다시 읽습니다 (예: Modify Table… 다음): SQL 보기에서는 DDL (뷰, 함수 등 모든 객체), Structure 보기에서는 구조. 예전 버전이 저장한 DDL 탭 가운데 어느 객체인지 모르는 탭은 다시 읽을 수 없습니다: 아이콘이 흐리게 바뀌고 "Reopen the DDL to reload" 라고 나옵니다 (TUI: `r` 을 누르면 상태 줄에 나옴).
- PostgreSQL 과 MySQL 은 모든 칸을 채웁니다. 다른 Data Source (드라이버 플러그인) 는 컬럼 · 키 · 인덱스를 보여 주고, 나머지는 "Not available for this data source" 라고 알려 줍니다.
- MySQL 은 계정에 권한이 있는 참조 테이블과 트리거만 보여 주므로, MySQL 에서 Referenced by 나 Triggers 가 비어 있으면 "None" 대신 "None visible to this account" 라고 표시합니다.
- 불러오기에 실패하면 이유를 보여 줍니다: **Retry** (TUI `r`), 또는 **Show SQL** (TUI `v`) — SQL 보기에는 전체 DDL 이 그대로 있습니다.
- 고치는 것은 이 보기가 아니라 **Modify Table…** (아래) 에서 합니다.

## 객체 만들기 · 고치기 (PostgreSQL, MySQL)

DataGrip 처럼: 우클릭 › **New ▸** Table… / View… / Materialized View… / Sequence… / Schema… / Database…, 테이블 › **Modify Table…** (`⌘F6`), **Rename…** (`⇧F6`). TUI: `⌃k` › New Table… / Modify Table… (`⌃F6`) / New View… / New Schema… / New Database… / New Sequence… / Rename… (`⇧F6`).
- 테이블 디자이너는 컬럼 (이름, 타입, NOT NULL, 기본값, 자동 번호 / identity, PK, 설명), 인덱스, 외래 키 (ON DELETE / ON UPDATE) 를 고칩니다. 고치는 동안 실행될 SQL 이 바로 보입니다: 새 테이블은 `CREATE TABLE`, 바꾸면 `ALTER` 문장들. 이름을 바꾼 컬럼은 키 · 인덱스 · 외래 키가 그대로 따라갑니다. 컬럼 타입 칸은 Data Source 의 타입을 칸 바로 아래 목록으로 추천합니다: 입력하면 걸러지고 (정확히 같은 것, 그 글자로 시작하는 것, 그 글자가 들어간 것 순), `↑` `↓` / Page Up / Page Down 으로 움직이고, `↩` 나 `⇥` 로 고르고, `⎋` 로 닫습니다. 목록에 없는 타입도 그대로 쳐 넣으면 됩니다.
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

## 콘솔 자동완성 (PostgreSQL, MySQL)

GUI 와 TUI 는 같은 엔진으로 자동완성하므로 같은 후보를 같은 순서로 보여 줍니다 (GUI 는 그다음 입력에 맞춰 퍼지로 걸러 냅니다). 무엇이 나오는지는 캐럿 위치에 따라 달라지고, 이름은 캐럿이 있는 문장에서만 가져옵니다:

- **문장 시작**에서는 문장 키워드 (`select`, `set`, `insert` …) 와 템플릿만 나옵니다: 새 줄에서 `se` 를 치면 위 문장의 컬럼이 아니라 `select`, `set` 이 나옵니다. 문장을 이어 갈 수도 있는 단어면 절 키워드가 먼저 나옵니다 (`an` 은 `and`, `de` 는 `desc`). 컬럼과 테이블은 들어갈 자리에서 나옵니다: `select`, `where`, `on`, `set`, `group by`, `order by`, `,` 뒤 …
- **정해진 다음 단어**: 뒤에 올 수 있는 단어가 몇 개뿐인 키워드 다음에는 그것만 나옵니다: `left` / `right` / `full` → `join`, `outer join`; `inner` / `cross` / `outer` → `join`; `order` / `group` → `by`, `partition` 은 `over (…)` / `window w as (…)` 의 첫 단어이거나 `)` 바로 뒤일 때만 → `by` (`partition` 이라는 컬럼은 평소대로 완성); `is` → `null`, `not null`, `not`, `true`, `false`, `distinct from`; `nulls` → `first`, `last`; 문장 시작(CTE 뒤 포함)의 `insert` → `into`, `delete` → `from` (MySQL 은 `ignore`, `low_priority` … 도; `on delete`, `after insert`, `grant insert` 는 평소대로 완성); `union` / `intersect` / `except` → `select`, `all`, `distinct`, `values` (PostgreSQL 은 `table` 도); 문장 시작의 `create` / `alter` / `drop` → 객체 종류 (`table`, `view`, `index`, `function`, `role`, `user` …; PostgreSQL 은 `materialized view`, `type`, `extension`, `policy`, `domain`, MySQL 은 `event` 도). 그 자리에는 컬럼이나 함수가 나오지 않습니다 (`left j` 는 `json_agg` 가 아니라 `join`).
- **스코프**: 컬럼은 캐럿 위치에서 보이는 테이블 · 서브쿼리 · CTE 의 것입니다: `from` / `join` 뒤에 CTE 이름, `cte.` 뒤에 그 컬럼, 서브쿼리 별칭과 그 select 목록의 이름 (`(select status as st from orders) s` → `s.st`), 중첩 `select` (`where` 나 select 목록 안) 에서 바깥 쿼리의 별칭, `using (…)` / `natural` 조인. CTE 본문과 `from` 의 서브쿼리는 자기 테이블만 봅니다 (`lateral` 이면 앞의 테이블도). 함수 호출 안의 `from` (`extract(year from …)`, `substring(… from …)`) 과 `is distinct from` 은 식의 일부라 그 뒤에 컬럼이 나옵니다.
- **조인**: `join` 뒤에는 문장의 테이블과 외래 키로 이어진 테이블 (어느 쪽 방향이든) 이 먼저 나오고, `join users u` 뒤에는 외래 키로 만든 조건 전체 (`on o.user_id = u.id`; 자기 자신과 조인하면 두 방향 모두), `on` 바로 뒤에는 `on` 없는 조건이 나옵니다.
- **절**: `group by` 는 select 목록에서 집계가 아닌 식을, `order by` 는 별칭을 먼저 보여 주고, `insert into t (…)` 안에서는 아직 쓰지 않은 컬럼만 나옵니다. `insert into t` 뒤에는 그 컬럼 목록과 채울 `values (…)` 가 나옵니다.
- **템플릿**: `sel` 은 `select * from `, `ins` 는 `insert into ` (그다음 위처럼 테이블 컬럼), `upd` 는 `update … set … = … where …`, `del` 은 `delete from … where …` 를 쓰고 자리표시자를 차례로 선택합니다 (GUI `⇥`; TUI 는 첫 자리가 선택되고 치면 바뀝니다). 키워드는 소문자로 씁니다.
- **`*` 펼치기**: select 목록의 `*` (또는 `t.*`) 바로 뒤에서 `⌃Space` 를 누르면 그 자리의 컬럼 목록을 DB 가 돌려주는 순서로 줍니다 (여러 테이블을 조인했으면 테이블 이름을 붙여서, `using (…)` / `natural` 이면 조인 컬럼을 맨 앞에 한 번). 컬럼을 알 수 없는 소스 (`from` 의 함수, 읽지 않은 테이블) 가 있으면 펼치지 않습니다: 빠진 목록은 다른 컬럼을 돌려주기 때문입니다.
- **함수**: 해당 DB 의 내장 함수와 DB 에 만든 함수가 시그니처와 함께 나옵니다. 함수 괄호 안에서는 GUI 의 시그니처 도움말 (TUI 는 캐럿 위의 힌트 줄) 이 함수를 보여 주고 캐럿 위치의 매개변수를 강조합니다. 사용자 함수는 오버로드를 모두 보여 줍니다.
- **모르는 이름**: 문장 스코프에 없는 테이블이나 컬럼에 밑줄을 긋습니다 (자동완성과 같은 스코프, 문장마다 따로). 그 위에서 `⌥↩` 를 누르면 가장 가까운 이름 목록이 나오고 고르면 바뀝니다 (TUI 에서 그 밖의 `⌥↩` 는 전처럼 실행입니다). 확실하지 않으면 표시하지 않습니다: CTE, 별칭, 함수, 카탈로그 (`pg_…`), 읽지 않은 다른 데이터베이스나 스키마, 컬럼을 알 수 없는 소스 (`from` 의 함수), 콘솔이 만드는 테이블, 이름이 아닌 SQL 단어 (`fetch next`, `window w`, `collate "C"`, `grouping sets`, MySQL 의 `with rollup`, `lock in share mode`, `straight_join` 과 `sql_…` 수식어), 인덱스 힌트, `select … into` 대상, PostgreSQL 시스템 컬럼 (`ctid`, `xmin` …), DDL, 메타데이터를 읽기 전. 이름은 Data Source 메타데이터를 읽은 때의 것이라, 다른 곳에서 테이블이나 컬럼을 만들었다면 Data Source 를 새로 고치면 표시가 사라집니다.

테이블 2,000 개를 읽어도 요청 하나는 2 ms 안에 답합니다 (p95; `cargo test --release -p qurry-shared --test complete_latency -- --ignored --nocapture`).

## 콘솔 값 추천 (PostgreSQL, MySQL)

자동완성이 테이블에 실제로 있는 값도 추천합니다: `status = '` · `status in ('paid', '` · `name like '` 뒤에는 그 컬럼의 값을, `data->>'` · `data->'meta'->>'` (MySQL `data->>'$.meta.`) 뒤에는 json 안의 키를 많이 나온 순으로 행 수와 함께 보여 줍니다. 따옴표를 치면 열립니다 (따옴표 없이는 `⌃Space` — 숫자는 그대로, 글자는 따옴표를 붙여 넣습니다). 앞쪽 5,000 행 (json 키는 2,000 행) 만 3초 제한의 읽기 전용 트랜잭션으로 읽고, 2분 동안은 다시 읽지 않습니다. 바이너리 컬럼과 json 전체 값, JOIN 의 `ON` 조건 안은 추천하지 않습니다 (반대편은 보통 컬럼이라 거기서는 컬럼이 자동완성되고, 그 뒤의 `WHERE` 에서는 다시 값을 추천합니다).

## 테이블 필터 자동완성

테이블 탭의 **WHERE** · **ORDER BY** 칸 (TUI: `w` 또는 `/` · `o`) 도 콘솔처럼 자동완성됩니다: 칸의 글을 `SELECT * FROM schema.table WHERE …` (또는 `ORDER BY …`) 로 읽으므로 그 테이블의 컬럼, 그다음 키워드와 함수가 나옵니다 (ORDER BY 에서 컬럼 뒤에는 `ASC` / `DESC`). 다른 테이블이나 스키마 이름은 나오지 않습니다. 단어를 치거나 `.` 뒤에서 열리고, 문자열 · 숫자 안에서는 열리지 않으며, 단어가 키워드나 컬럼 이름 그대로가 되면 닫힙니다 — 그래서 `is null` 뒤의 `↩` 는 필터를 적용합니다. 목록이 열려 있으면 `↑` `↓` `PgUp` `PgDn` 이동, `↩` / `⇥` 넣기, `⎋` 목록 닫기. 닫혀 있으면 `⌃Space` (TUI 는 `⇥` 도) 로 커서에서 목록을 엽니다 — 공백 뒤나 단어가 완성된 뒤에도 열리고, 빈 자리에서는 그 테이블의 컬럼이 먼저, 그다음 키워드가 나옵니다. `↩` 는 필터 적용, `⎋` 는 칸을 원래대로 되돌립니다. 컬럼 값 추천은 여기서는 하지 않습니다.

두 칸 (GUI · TUI) 의 글은 콘솔의 색으로 그려집니다: 키워드, 함수 (`(` 앞의 이름), 숫자, 문자열, 주석. 단어 연산자 `IN` `IS` `NOT` `NULL` `AND` `OR` `LIKE` `BETWEEN` 은 여기서 키워드이고, 0.3.3 부터 GUI 콘솔에서도 키워드 색입니다 (전에는 콘솔이 `=` · `<` 와 같은 회색으로 그렸고, 기호 연산자는 그 색 그대로입니다).

## 따옴표와 괄호

`'` `"` `(` `[` 를 치면 커서 뒤에 닫는 글자가 함께 들어갑니다 — GUI 콘솔, 테이블 탭의 **WHERE** · **ORDER BY** 칸, TUI 콘솔과 `w` / `o` 입력 칸 모두. 공백 · 줄 끝 · `;:.,=}])>` 앞에서만 그렇고, 따옴표는 글자나 숫자 바로 뒤에서는 혼자 들어갑니다 (`it's` 는 친 그대로). 닫는 글자가 이미 커서 바로 뒤에 있으면 그 글자를 치면 건너가고, 빈 짝 사이에서 `⌫` 는 둘 다 지우며, 선택한 글 위에서 여는 글자를 치면 선택을 감쌉니다 (선택은 그대로). GUI 콘솔 밖에서는 문자열 · 주석 안에서 닫는 글자를 넣지 않습니다. GUI 콘솔은 여기서 Monaco 를 따르고 `{` 도 짝을 맞춥니다.

닫히지 않은 따옴표 · 괄호 (또는 여는 짝이 없는 닫는 괄호) 는 치는 동안 콘솔에 표시됩니다: GUI 는 그 글자부터 그 줄 끝까지 빨간 밑줄 (마우스를 올리면 "Unclosed '('" 처럼 무엇이 열려 있는지), TUI 는 그 글자에 빨간 밑줄과 줄 번호를 빨갛게, 커서가 그 줄에 있으면 편집기 아래 테두리에 무엇이 열려 있는지 나옵니다. 문자열 · 주석 · PostgreSQL 달러 인용 (`$$ … $$`, `$tag$ … $tag$`) 은 Qurry 가 문장을 나누는 규칙 그대로 읽으므로 그 안의 `(` 는 세지 않고, 괄호 짝은 한 문장 안에서 맞춥니다.

## 쿼리 히스토리 · 스니펫

쿼리 히스토리와 스니펫은 설정 폴더에 저장되고 GUI · TUI 가 함께 씁니다 (`PASSWORD '…'` / `IDENTIFIED BY '…'` 의 비밀번호는 가립니다). GUI: `⌥⌘e` (History / Snippets 탭, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## 실행 계획

GUI · TUI 콘솔 (그리고 MCP `explain` 도구) 은 종류마다 맞는 문장을 보냅니다: PostgreSQL 은 `EXPLAIN (FORMAT JSON)` 이고 트리로 그립니다. MySQL · DuckDB · ClickHouse 는 `EXPLAIN`, SQLite 는 `EXPLAIN QUERY PLAN` 이고 계획을 행으로 보여 줍니다. SQLite · ClickHouse 에는 Explain Analyze 가 없습니다. SQL Server · Oracle · MongoDB · Redis · 벡터 DB 는 Qurry 에서 Explain 이 안 됩니다: 콘솔이 그렇다고 말하고 아무것도 보내지 않으니, Explain 없이 실행하세요.

PostgreSQL 에서는 노드마다 자기 몫의 시간 (Explain Analyze) 또는 비용을 보여 주고, 볼 만한 것을 표시합니다 — 큰 테이블의 Seq Scan, 10배 이상 틀린 행 수 추정, 대부분의 행을 버리는 필터, 디스크로 넘친 정렬 · 해시. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze 는 문장을 실제로 실행합니다. PostgreSQL · MySQL 에서는 트랜잭션 안에서 실행하고 되돌립니다 — 이미 트랜잭션이 열려 있으면 세이브포인트까지만 — 그래서 테이블 변경은 되돌려집니다 (MySQL 에서는 InnoDB 같은 트랜잭션 테이블만이고, MyISAM · MEMORY 는 아닙니다). 되돌릴 수 없는 것 (시퀀스 값, `dblink_exec`, `SELECT … INTO OUTFILE`) 과 `WHERE` 없는 `UPDATE` · `DELETE` 는 먼저 묻습니다 ([안전장치](safety.md)).

---

[← 사용설명서](README.md)
