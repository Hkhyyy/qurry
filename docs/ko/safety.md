# 안전장치

[English](../safety.md) | 한국어

- **읽기 전용 Data Source** 는 화면만이 아니라 엔진이 지킵니다: 모든 콘솔, 테이블 편집기, Redis, 내보내기에서 읽는 문장만 실행합니다 (트랜잭션 제어와 `SET` 은 되지만 읽기 전용을 끄는 것은 안 됨).
- **트랜잭션**: 직접 연 트랜잭션 (Manual 모드, 또는 Auto 모드의 `BEGIN`) 은 결과를 나눠 받는 동안 몰래 커밋되거나 되돌려지지 않습니다. 커밋 안 한 변경이 있을 때 Manual → Auto 로 바꾸면 먼저 묻고, 탭 닫기 · 연결 끊기 · 데이터베이스 바꾸기 · 종료도 되돌리기 전에 묻습니다. 열린 트랜잭션이 있는 Data Source 는 자동으로 연결이 끊기지 않습니다.
- **확인**: `WHERE` 없는 `UPDATE`/`DELETE`, `DROP`, `TRUNCATE`, Redis `FLUSHALL`/`FLUSHDB`/`DEL` 은 실행 전에 묻습니다 (GUI, TUI).
- **Production Data Source** 는 바꾸는 문장이 몇 행을 건드리는지 보여 주고 실행 전에 묻습니다 ([Data Source](data-sources.md)).
- **운영 작업 기록**: Production Data Source 에서 실행한 바꾸는 문장 (콘솔, 테이블 편집기, 테스트 데이터, 복사) 을 시각 · Data Source · 바뀐 행 수 · 커밋 전이었는지와 함께 남기고, 뒤의 COMMIT / ROLLBACK 도 남깁니다. GUI: `⇧⌘A` › Production Log… · TUI: `⌃k` › Production Log…. 설정 폴더에 저장됩니다 (`audit.jsonl`, 본인만 읽을 수 있음, 최근 2,000개).
- **되돌리기 SQL**: Production Data Source (PostgreSQL, MySQL) 에서 테이블 하나에 대한 단순한 `UPDATE` / `DELETE` 를 실행하기 직전에 바뀔 행을 떠 두고 (최대 1,000 행), 되돌리는 SQL 을 기록에 남깁니다. 저절로 실행하지 않습니다: 기록에서 콘솔로 열어 확인하고 실행하세요 (그때도 Production 확인을 거칩니다). 테이블에 PK 가 없거나 문장이 PK 를 바꾸면 남기지 않습니다.
- **Explain Analyze** 를 데이터를 바꾸는 문장에 쓰면 트랜잭션 안에서 실행하고 되돌리므로 (PostgreSQL, MySQL) 아무것도 바뀌지 않습니다.
- **Auto 모드의 스크립트** 는 문장 하나씩 실행합니다. 중간에 실패하면 앞에서 이미 커밋된 문장이 몇 개인지 출력에 알려 줍니다.
- **테이블 편집기** 는 모든 편집을 트랜잭션 하나로 저장하고, `UPDATE`/`DELETE` 하나가 정확히 한 행만 바꿔야 합니다. 아니면 아무것도 저장하지 않습니다.
- **Data Source 파일 가져오기** 는 무엇이 바뀌는지 먼저 보여 줍니다 (예: "turns Read-only OFF", 새 시작 스크립트).
- **쿼리 히스토리** (GUI) 는 `CREATE/ALTER USER … PASSWORD '…'`, `IDENTIFIED BY '…'`, Redis `AUTH` 의 비밀번호를 가립니다.
- **릴리스** 는 `main` 에서만, 커밋 SHA 로 고정한 GitHub Actions 로 빌드하고 `SHA256SUMS` 와 빌드 출처 증명 (provenance attestation) 을 붙입니다. 업데이트 서명 키는 빌드와 분리된 서명 단계에서만 씁니다. GUI 는 업데이트 묶음의 서명을, `qurry-tui --update` 는 `SHA256SUMS` 의 서명 (같은 키) 과 체크섬을 확인한 뒤 설치합니다.

---

[← 사용설명서](README.md)
