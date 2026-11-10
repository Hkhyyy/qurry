# MCP 서버 (AI 도구)

[English](../mcp.md) | 한국어

Claude Code 같은 AI 도구가 데이터베이스를 둘러보고 조회할 수 있게 해 줍니다. 기본은 읽기 전용입니다.

```bash
claude mcp add qurry -- qurry-mcp                   # 읽기 전용 (기본)
claude mcp add qurry-rw -- qurry-mcp --allow-write  # execute 도구 추가
```

**AI 에게 보이는 Data Source** — Data Source 마다 **MCP** 설정이 있습니다 (GUI: Data Source › Options › MCP · TUI: Data Source 폼의 `MCP` 칸).
- **Off**: 목록에 없고, MCP 서버가 접속하지도 비밀번호를 읽지도 않습니다. 새 Data Source 는 여기서 시작합니다.
- **Read**: 둘러보기와 읽기 전용 쿼리.
- **Read & write**: `qurry-mcp` 를 `--allow-write` 로 띄우면 `execute` 도. 읽기 전용 Data Source 와 **Production** Data Source 는 쓰지 않고 Read 에 머뭅니다 (GUI 는 이 선택지를 회색으로 막고, TUI 는 read 로 저장합니다). AI 의 변경은 앱이 사람에게 묻는 바뀔 행 수 확인을 건너뛰기 때문입니다.

이 설정이 생기기 전에 만든 Data Source 는 예전처럼 동작합니다 (읽기, `--allow-write` 면 쓰기). **Production** 만은 MCP 를 켜 주기 전까지 보이지 않습니다.

| 도구 | 설명 |
|---|---|
| `list_data_sources` | MCP 에 보이는 연결 (종류, 호스트 — URI 안의 비밀번호는 뺌, 기본 데이터베이스, 읽기 전용, Production, MCP 권한, SSH, 그리고 `dba_report`: 그 연결에 `dba_report` 가 답하는지) |
| `list_databases` / `list_schemas` / `list_objects` | 트리 둘러보기 (Redis: 키와 타입) |
| `describe_table` | 컬럼, 인덱스, 제약 조건 |
| `get_erd` | 테이블과 그 기본 키, 테이블 사이의 외래 키를 JSON 과 Mermaid `erDiagram` 으로 (행 데이터는 없음). `schemas` 를 주지 않으면 PostgreSQL 은 `public`, MySQL 은 그 데이터베이스입니다. 외래 키가 가리키는 다른 스키마의 테이블도 함께 옵니다. 테이블은 200개까지: 넘으면 `note` 가 전체 개수를 알려 주고, 빠진 테이블로 가는 키는 뺍니다 |
| `get_ddl` | DDL (Redis: 키를 다시 만드는 명령 — 요소 1,000개까지, 더 큰 키는 `redis_get`) |
| `search_objects` | 객체 이름으로 퍼지 검색 (Go to Database Object). Data Source 마다 보이는 데이터베이스 전체에서 |
| `query` | 읽기 전용 쿼리 (기본 100행, 최대 1000행) |
| `explain` | 실행 계획 (ANALYZE 선택). PostgreSQL 은 content 항목 두 개로 답합니다: 예전과 같은 계획 텍스트, 그리고 JSON `{"plan_analysis": …}` — 계획 트리와 단계마다 예상 비용에서 차지하는 몫, 자기 비용, 예상 행 수, 경고 (큰 테이블을 Seq Scan 으로 읽음, 정렬이나 해시가 디스크로 넘침 …). 분석은 ANALYZE 없이 따로 돌린 `EXPLAIN (FORMAT JSON)` 에서 나오므로, `analyze` 를 줘도 문장은 한 번만 실행되고 몫은 비용 기준입니다 (`"basis": "cost"`). 이 두 번째 실행이 실패하면 텍스트만 옵니다. MySQL 과 다른 종류는 예전처럼 항목 하나로 답합니다 |
| `full_text_search` | 모든 테이블에서 값 찾기 |
| `dba_report` | Database Admin 리포트 하나를 id 로 (PostgreSQL / MySQL): `sessions`, `locks`, `slow_queries`, `table_sizes`, `table_bloat`, `index_bloat`, `fragmentation`, `unused_indexes`, `duplicate_indexes`, `fk_without_index`, `vacuum`, `vacuum_progress`, `wraparound`, `replication`, `server`, `settings`, `roles`, `grants` — 종류마다 있는 리포트가 다르고, 그 Data Source 에 없는 id 를 주면 있는 목록을 알려 줍니다. 행 (최대 1000행), 제목, 읽는 법을 돌려줍니다. 아래 참고 |
| `redis_scan` / `redis_get` | Redis 키 목록 / 키 상세 |
| `export_csv` | 읽기 전용 쿼리 결과를 `~/Downloads/qurry-exports` 에 CSV 로 저장 (파일 이름만, 덮어쓰지 않음, 최대 60초 · 512 MB) |
| `list_rag_indexes` / `semantic_search` | 앱에서 만든 RAG 인덱스 / 질문과 뜻이 가장 가까운 행을 점수와 함께 |
| `execute` | `--allow-write` 이고 **Read & write** 로 둔 Data Source 만. 허용 목록 방식입니다: 항목 하나에 `INSERT` 하나, 또는 `WHERE` 있는 `UPDATE` / `DELETE` 하나 (PostgreSQL / MySQL 은 항목마다 prepared statement 로 실행해서 두 번째 문장은 서버가 거부합니다). MongoDB 는 `insertOne` / `insertMany` 와 조건이 있는 `updateOne` / `updateMany` / `replaceOne` / `deleteOne` / `deleteMany`, 벡터 DB 는 점 추가와 id 로 지우기, Redis 는 읽기와 만료 옵션 없는 키 하나 단위의 쓰기. PostgreSQL / MySQL 은 모든 항목을 트랜잭션 하나로, 다른 종류는 하나씩 실행합니다. 그 밖 (DDL, `MERGE`, `TRUNCATE`, 스크립트, 컬렉션 지우기 …) 은 거부합니다 — 사용자에게 Qurry 에서 실행해 달라고 하세요. `WHERE` / 조건 규칙은 실수만 막습니다: `WHERE 1=1` 이나 `{$comment: "x"}` 도 모든 행에 맞으니, AI 가 바꿀 수 있는 범위는 데이터베이스 사용자 권한으로 제한하세요 |

**읽기 전용을 지키는 방법**
- PostgreSQL / MySQL: 정확히 한 문장, SELECT / WITH / VALUES / TABLE / SHOW / EXPLAIN / DESCRIBE 로 시작해야 하고, 60초 제한이 걸린 READ ONLY 트랜잭션 안에서 실행한 뒤 되돌립니다. 문장은 서버 자체의 주석 규칙으로 나눕니다 (MySQL `--x` 와 `/*! … */`, MariaDB `/*M! … */`, 백쿼트 이름, PostgreSQL 중첩 주석). MySQL 은 문장 하나만 받는 prepared-statement 프로토콜로 실행합니다. `COMMIT; DELETE …`, `WITH d AS (DELETE …) SELECT …`, `EXPLAIN ANALYZE DELETE`, `set_config(…read_only…)`, `SELECT … INTO OUTFILE`, 트랜잭션 밖에 영향을 주는 함수 (`pg_terminate_backend`, `pg_read_file`, `dblink`, `load_file`, `sleep`, …) 는 거부합니다.
- Redis: 읽기 명령만. `CONFIG` (비밀번호가 보일 수 있음) 와 `KEYS` (`redis_scan` 을 쓰세요) 는 안 됩니다. 명령 하나는 요소를 1,000개까지 읽습니다: `LRANGE 0 999` 나 `LRANGE -10 -1` 같은 범위, `ZRANGEBYSCORE … LIMIT`, `XRANGE … COUNT` (`LIMIT 0 1 LIMIT 0 -1` 처럼 같은 옵션을 두 번 쓰면 거부합니다: Redis 는 마지막 것을 씁니다). 키 하나를 통째로 읽는 명령 (`HGETALL`, `SMEMBERS`, `SUNION`, `LRANGE 0 -1`, `COUNT` 없는 `XRANGE`, …) 은 키의 요소가 1,000개 이하 (그리고 16 MB 이하) 일 때만 실행합니다 — 그보다 크면 `HSCAN` / `SSCAN`, 범위를 정한 명령, 또는 `redis_get` 을 쓰세요. `execute` 로 보낸 읽기 명령도 똑같이 확인합니다.
- 드라이버 Data Source (SQL Server, DuckDB, ClickHouse, MongoDB, 벡터 DB) 는 READ ONLY 트랜잭션이 없어서 더 엄격하게 봅니다: 한 문장만 (SQL Server 배치에 이어 쓴 두 번째 문장도 안 됨), `SELECT … INTO` 안 됨, 파일이나 다른 서버를 읽는 함수 (`OPENROWSET`, ClickHouse `url()` / `file()` / `remote()` …, DuckDB `read_text()` …) 안 됨, 벡터 DB 는 search / query / list 요청만, MongoDB 는 `$out` / `$merge` 안 됨. DuckDB 는 외부 접근을 끄고 열고, ClickHouse 읽기는 `readonly=2` 로 실행합니다 (사용자가 이미 읽기 전용이면 그대로). MCP 서버에만 해당하고, 앱에서 읽기 전용 DuckDB Data Source 는 내 컴퓨터의 파일을 읽을 수 있습니다.
- 그래도 MCP 서버에는 **최소 권한 DB 사용자** (읽기 전용 역할, superuser · FILE 권한 없음) 를 주세요. 위 검사는 두 번째 방어선이지 DB 권한을 대신하지 않습니다.

**Database Admin 리포트 (`dba_report`)** — 앱의 Database Admin 과 같은 리포트를 Qurry 가 쓴 그대로 실행합니다: 클라이언트가 보낸 SQL 은 실행하지 않습니다. 이것으로는 아무것도 바뀌지 않습니다: Cancel Query, Terminate Session, 생성 SQL 이 없어서, 리포트가 가리키는 해결책 (CREATE / DROP INDEX, VACUUM, REINDEX …) 은 사람이 Qurry 에서 실행할 텍스트로만 돌아옵니다.
- 문장 텍스트 (`sessions`, `locks`, `slow_queries`, `vacuum_progress`) 안의 문자열 · 숫자 리터럴은 모든 Data Source 에서 `?` 로 바꿉니다. `where email = 'a@b.c'` 는 `where email = ?` 로 보입니다. 이름, 주석, `$1` 자리표시자, MySQL 이 이미 쓴 `?` 는 그대로입니다.
- **Production** Data Source 에서는 사람이 Qurry 에서 그 Data Source 에 허용해 준 경우에만 답합니다. 처음에는 꺼져 있습니다. 리포트에는 누가 어디서 접속해 무엇을 실행하는지, 서버 설정, 역할과 권한이 나옵니다. MCP 에 보이는 다른 Data Source 는 따로 허용하지 않아도 답합니다. 예전 Qurry 가 그 Data Source 를 저장하면 이 허용이 빠져서, 사람이 다시 허용할 때까지 꺼집니다.

무엇을 막고 무엇을 막지 못하는지 (데이터를 통한 프롬프트 인젝션 포함): [MCP 위협 모델](mcp-security.md).

옵션: `--config-dir DIR` (다른 설정 폴더 사용), `--verbose` (엔진 로그를 stderr 로).

---

[← 사용설명서](README.md)
