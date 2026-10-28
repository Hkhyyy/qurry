# Vector databases and AI search (RAG)

English | [한국어](ko/vector-and-rag.md)

**Qdrant, Chroma, Milvus** — the explorer lists collections with their vector size and distance and the payload / metadata fields (guessed from 50 points); opening one shows its points as rows (long vectors are shortened). The console sends REST requests: a `METHOD /path` line and an optional JSON body on the next lines (unquoted keys and trailing commas are fine). Run the request under the cursor as usual.

```
POST /collections/docs/points/search          # Qdrant
{vector: [0.12, 0.5, 0.33], limit: 5, with_payload: true}

POST /collections/docs/query                  # Chroma (paths are relative to the database; names work instead of ids)
{query_embeddings: [[0.12, 0.5, 0.33]], n_results: 5}

POST /entities/search                         # Milvus (after /v2/vectordb; dbName is filled in)
{collectionName: 'docs', data: [[0.12, 0.5, 0.33]], limit: 5, outputFields: ['*']}
```

The API key goes in the password field (Milvus: user + password, or the password alone as a token).

**pgvector** — `vector`, `halfvec` and `sparsevec` values show as PostgreSQL prints them. In a table with a vector column, right-click a row › **Find Similar Rows by …** (TUI: `⌃k` › Find Similar Rows) writes the nearest-neighbour query for that row, using the operator of the column's HNSW / IVFFlat index so the index is used.

**AI search (RAG)** — search a table by meaning instead of by words.
1. GUI: right-click a table › **AI Search (RAG) — Index This Table…** (or a data source › Tools › AI Search). TUI: select the table, `⌃k` › New RAG Index….
2. Choose the column that identifies a row, the text columns to embed, the embedding model and where to keep the vectors: a table in PostgreSQL with pgvector (created for you, with an HNSW index), or a Qdrant / Chroma / Milvus collection.
   - **Ollama** (default) runs on your computer — nothing leaves it. `ollama pull nomic-embed-text` first; another address via the Base URL field or `OLLAMA_HOST`.
   - **OpenAI** sends each row's text to OpenAI. The API key is stored encrypted like passwords (or set `OPENAI_API_KEY`); any OpenAI-compatible server works through Base URL / `OPENAI_BASE_URL`.
3. **Build** embeds the rows in batches (progress in the status bar, cancel any time). Building again updates rows in place.
4. Ask in plain words — "customers unhappy about late delivery" — and get the closest rows first with a score (1 = same meaning). From Claude, the MCP server's `semantic_search` tool does the same.

Index definitions are kept in `rag.json` in the config folder (shared by the GUI, TUI and MCP server). Writing vectors goes through the data source like any other statement, so a read-only data source can't be used as the store.

---

[← User guide](README.md)
