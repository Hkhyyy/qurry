# MCP 서버 (AI 도구)

[English](../mcp.md) | 한국어

Claude Code 같은 AI 도구가 데이터베이스를 둘러보고 조회할 수 있게 해 줍니다. 기본은 읽기 전용입니다.

```bash
claude mcp add qurry -- qurry-mcp                   # 읽기 전용 (기본)
claude mcp add qurry-rw -- qurry-mcp --allow-write  # execute 도구 추가
```

| 도구 | 설명 |
|---|---|
| `list_data_sources` | 설정된 연결 (종류, 호스트, 기본 데이터베이스, 읽기 전용, SSH) |
| `list_databases` / `list_schemas` / `list_objects` | 트리 둘러보기 (Redis: 키와 타입) |
| `describe_table` | 컬럼, 인덱스, 제약 조건 |
| `get_ddl` | DDL (Redis: 키를 다시 만드는 명령) |
| `search_objects` | 객체 이름으로 퍼지 검색 (Go to Database Object). Data Source 마다 보이는 데이터베이스 전체에서 |
| `query` | 읽기 전용 쿼리 (기본 100행, 최대 1000행) |
| `explain` | 실행 계획 (ANALYZE 선택) |
| `full_text_search` | 모든 테이블에서 값 찾기 |
| `redis_scan` / `redis_get` | Redis 키 목록 / 키 상세 |
| `export_csv` | 읽기 전용 쿼리 결과를 `~/Downloads/qurry-exports` 에 CSV 로 저장 (파일 이름만, 덮어쓰지 않음) |
| `list_rag_indexes` / `semantic_search` | 앱에서 만든 RAG 인덱스 / 질문과 뜻이 가장 가까운 행을 점수와 함께 |
| `execute` | `--allow-write` 일 때만. 문장들을 트랜잭션 하나로 실행. 읽기 전용 Data Source 는 거부 |

**읽기 전용을 지키는 방법**
- PostgreSQL / MySQL: 정확히 한 문장, SELECT / WITH / VALUES / TABLE / SHOW / EXPLAIN / DESCRIBE 로 시작해야 하고, 60초 제한이 걸린 READ ONLY 트랜잭션 안에서 실행한 뒤 되돌립니다. 문장은 서버 자체의 주석 규칙으로 나눕니다 (MySQL `--x` 와 `/*! … */`, MariaDB `/*M! … */`, 백쿼트 이름, PostgreSQL 중첩 주석). MySQL 은 문장 하나만 받는 prepared-statement 프로토콜로 실행합니다. `COMMIT; DELETE …`, `WITH d AS (DELETE …) SELECT …`, `EXPLAIN ANALYZE DELETE`, `set_config(…read_only…)`, `SELECT … INTO OUTFILE`, 트랜잭션 밖에 영향을 주는 함수 (`pg_terminate_backend`, `pg_read_file`, `dblink`, `load_file`, `sleep`, …) 는 거부합니다.
- Redis: 읽기 명령만. `CONFIG` (비밀번호가 보일 수 있음) 와 `KEYS` (`redis_scan` 을 쓰세요) 는 안 됩니다.
- 그래도 MCP 서버에는 **최소 권한 DB 사용자** (읽기 전용 역할, superuser · FILE 권한 없음) 를 주세요. 위 검사는 두 번째 방어선이지 DB 권한을 대신하지 않습니다.

옵션: `--config-dir DIR` (다른 설정 폴더 사용), `--verbose` (엔진 로그를 stderr 로).

---

[← 사용설명서](README.md)
