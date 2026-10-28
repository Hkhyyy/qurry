# 벡터 DB 와 AI 검색 (RAG)

[English](../vector-and-rag.md) | 한국어

**Qdrant, Chroma, Milvus** — 탐색기에 컬렉션이 벡터 크기 · 거리 방식, payload / metadata 필드 (점 50개를 보고 짐작) 와 함께 나옵니다. 컬렉션을 열면 점들이 행으로 보입니다 (긴 벡터는 줄여서). 콘솔은 REST 요청을 보냅니다: `METHOD /path` 한 줄, 그 아래 줄에 JSON 본문 (선택. 따옴표 없는 키와 끝 쉼표도 됩니다). 커서가 있는 요청을 평소처럼 실행합니다.

```
POST /collections/docs/points/search          # Qdrant
{vector: [0.12, 0.5, 0.33], limit: 5, with_payload: true}

POST /collections/docs/query                  # Chroma (경로는 데이터베이스 기준, id 대신 이름도 됨)
{query_embeddings: [[0.12, 0.5, 0.33]], n_results: 5}

POST /entities/search                         # Milvus (/v2/vectordb 뒤 경로, dbName 은 채워 줌)
{collectionName: 'docs', data: [[0.12, 0.5, 0.33]], limit: 5, outputFields: ['*']}
```

API 키는 비밀번호 칸에 넣습니다 (Milvus: 사용자 + 비밀번호, 또는 비밀번호만 토큰으로).

**pgvector** — `vector`, `halfvec`, `sparsevec` 값은 PostgreSQL 이 출력하는 그대로 보입니다. 벡터 컬럼이 있는 테이블에서 행 우클릭 › **Find Similar Rows by …** (TUI: `⌃k` › Find Similar Rows) 는 그 행과 가장 가까운 행을 찾는 쿼리를 써 줍니다. 컬럼의 HNSW / IVFFlat 인덱스 연산자를 써서 인덱스를 탑니다.

**AI 검색 (RAG)** — 테이블을 단어가 아니라 뜻으로 찾습니다.
1. GUI: 테이블 우클릭 › **AI Search (RAG) — Index This Table…** (또는 Data Source › Tools › AI Search). TUI: 테이블을 고르고 `⌃k` › New RAG Index….
2. 행을 구분하는 컬럼, 임베딩할 글 컬럼, 임베딩 모델, 벡터를 둘 곳을 고릅니다: pgvector 가 있는 PostgreSQL 의 테이블 (HNSW 인덱스까지 만들어 줌) 또는 Qdrant / Chroma / Milvus 컬렉션.
   - **Ollama** (기본) 는 이 컴퓨터에서 돌아서 아무것도 밖으로 나가지 않습니다. 먼저 `ollama pull nomic-embed-text`. 다른 주소는 Base URL 칸이나 `OLLAMA_HOST`.
   - **OpenAI** 는 행마다 글을 OpenAI 로 보냅니다. API 키는 비밀번호처럼 암호화해 저장합니다 (또는 `OPENAI_API_KEY`). OpenAI 호환 서버는 Base URL / `OPENAI_BASE_URL` 로 씁니다.
3. **Build** 는 행을 묶음으로 임베딩합니다 (진행 상황은 상태 표시줄, 언제든 취소). 다시 Build 하면 행을 그 자리에서 갱신합니다.
4. 평범한 말로 물어봅니다 — "배송이 늦어서 불만인 고객" — 가장 가까운 행이 점수 (1 = 같은 뜻) 와 함께 먼저 나옵니다. Claude 에서는 MCP 서버의 `semantic_search` 도구가 같은 일을 합니다.

인덱스 정의는 설정 폴더의 `rag.json` 에 있습니다 (GUI · TUI · MCP 서버가 함께 씀). 벡터 쓰기는 다른 문장처럼 Data Source 를 거치므로, 읽기 전용 Data Source 는 저장소로 쓸 수 없습니다.

---

[← 사용설명서](README.md)
