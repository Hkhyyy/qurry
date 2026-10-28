# Data Source

[English](../data-sources.md) | 한국어

Qurry 가 다루는 데이터베이스, 연결 방법 (SSH, SSL, 옵션), 덜 흔한 데이터베이스의 드라이버를 추가하는 방법입니다.

## 지원하는 데이터베이스

| 데이터베이스 | 기능 |
|---|---|
| PostgreSQL | 탐색기 트리 (데이터베이스, 스키마, 테이블, 뷰, 시퀀스, 루틴), 쿼리 콘솔 (서버 커서로 나눠 받기, 취소, 수동 트랜잭션), 데이터 편집, 실행 계획, DDL, 전체 텍스트 검색 |
| MySQL / MariaDB | 위와 같음. MySQL 은 데이터베이스가 곧 스키마라서 테이블이 데이터베이스 바로 아래에 보입니다 |
| Redis | Medis 같은 브라우저 (타입 · TTL 이 붙은 키 목록, 타입별 값 편집기), redis-cli 같은 터미널, **Standalone / Sentinel / Cluster** |

연결 옵션:
- **SSH 터널** (키 또는 비밀번호). 호스트 키는 `~/.ssh/known_hosts` 로 확인합니다. 키가 바뀌었으면 거부하고, 처음 보는 서버는 SHA256 지문을 보여 주며 **Trust host key** 를 고를 때까지 멈춥니다 (GUI: Data Source 창, TUI: 폼에서 `⌃y` 또는 `⌃k › Trust SSH Host Key`).
- **SSL/TLS**: Require / Verify CA / Full Verification, CA 와 클라이언트 인증서는 선택. PostgreSQL · MySQL · Redis 에서 됩니다.
- **Options 탭**: 읽기 전용, 트랜잭션 방식과 격리 수준, 시간대, keep-alive, 오래 안 쓰면 자동 연결 끊기, 단일 데이터베이스 모드, 시작 스크립트.
- **Production**: 데이터를 바꾸는 문장을 실행하기 전에 바뀔 행 수를 세어 (같은 테이블 · WHERE 로 `SELECT count(*)`, 아무것도 바꾸지 않음) 보여 주고 묻습니다. 셀 수 없는 문장 (`UPDATE … FROM`, `DELETE … USING`, CTE) 도 묻습니다. 구조를 바꾸는 문장 (DDL) 은 "schema change" 로 보입니다. GUI: Options 탭 · TUI: Data Source 폼의 `Production` 토글.

> Redis Sentinel · Cluster 노드는 자기 주소를 알려 주는 방식이라 SSH 터널과 같이 쓸 수 없습니다.

## 드라이버 (필요할 때 추가)

처음에는 아무것도 더 설치하지 않습니다. Data Source 종류로 아래 중 하나를 고르면 그 드라이버를 **Install** 하라고 안내합니다. 드라이버는 지금 Qurry 버전의 GitHub 릴리스에서 받는 작은 프로그램으로, 서명된 `SHA256SUMS` (앱 업데이트와 같은 키) 로 확인하고 설정 폴더의 `drivers/` 에 둡니다.

| 데이터베이스 | 참고 |
|---|---|
| SQL Server | SQL Server 2017+ (더 옛 버전도 경고와 함께 됨). `dbo` 와 다른 스키마 |
| Oracle | 무료 [Oracle Instant Client](https://www.oracle.com/database/technologies/instant-client.html) 가 필요합니다. 그 폴더를 Data Source 에 지정 (또는 `ORACLE_CLIENT_LIB_DIR`). Oracle 19c+ (11g 도 경고와 함께 됨). 서비스 이름은 Database 칸에 |
| SQLite · DuckDB | 호스트 · 포트 대신 파일 (DuckDB: 비우면 메모리) |
| ClickHouse | HTTP (포트 8123) |
| MongoDB | 콘솔에서 셸 문법: `db.users.find({age: {$gt: 30}}).sort({age: -1}).limit(10)`, `aggregate`, `countDocuments`, `insertMany`, `updateMany` … 또는 JSON 명령. 필드는 문서 50개를 보고 짐작합니다 |
| Qdrant · Chroma · Milvus | 컬렉션이 테이블입니다. 콘솔은 REST 요청을 받습니다 — [벡터 DB](vector-and-rag.md) 참고 |

연결 시험은 서버 버전을 보여 주고, Qurry 가 시험하는 버전 (PostgreSQL 12, MySQL 5.7, MariaDB 10.5, SQL Server 2017, MongoDB 5, ClickHouse 22, Oracle 19c — CI 가 이 버전들로 돕니다) 보다 낮으면 경고합니다. 읽기 전용 Data Source 는 드라이버에서도 지켜집니다: MongoDB 는 `find` / `aggregate` (`$out` / `$merge` 없이) / `count` / `distinct` 만, 벡터 DB 는 `GET` 과 검색 요청만 실행합니다.

---

[← 사용설명서](README.md)
