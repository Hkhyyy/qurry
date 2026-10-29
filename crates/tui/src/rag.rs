//! AI Search (RAG) (⌃K): pick an index → question → results in a new console's result pane. Build (progress on the bottom line), new index from a tree table, OpenAI key
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind};
use crate::grid::Grid;
use crate::migrate::Flow;
use crate::tree::NodeKind;
use qurry_core::rag::{RagIndex, default_model};
use qurry_core::sql;
use qurry_core::store::DbKind;
use tui_textarea::TextArea;

fn palette(title: &str, items: Vec<PItem>) -> Popup {
    Popup::Palette(Palette { title: title.into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() })
}

impl App {
    /// Pick a saved index and go to flow
    fn rag_pick(&mut self, title: &str, flow: Flow) {
        let list = match self.state.rag_indexes() {
            Ok(l) if !l.is_empty() => l,
            Ok(_) => {
                self.status = "No RAG index yet — select a table in the explorer, then ⌃K › New RAG Index".into();
                return;
            }
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        let items = list
            .into_iter()
            .map(|x| PItem { label: x.name.clone(), hint: if x.rows > 0 { format!("{}.{} · {} rows", x.schema, x.table, x.rows) } else { format!("{}.{} · not built", x.schema, x.table) }, action: PAction::Pick(x.id) })
            .collect();
        self.flow = Some(flow);
        self.popup = Some(palette(title, items));
    }

    /// ⌃K › AI Search (RAG)…
    pub fn rag_search_start(&mut self) {
        self.rag_pick("AI Search — pick an index", Flow::RagSearch);
    }

    /// ⌃K › Build RAG Index…
    pub fn rag_build_start(&mut self) {
        self.rag_pick("Build (embed rows of) — pick an index", Flow::RagBuild);
    }

    /// What was picked in the picker window (RAG step)
    pub fn rag_flow(&mut self, flow: Flow, value: String) {
        match flow {
            Flow::RagSearch => {
                let Ok(idx) = self.state.rag_indexes().map(|l| l.into_iter().find(|x| x.id == value)) else { return };
                let Some(idx) = idx else { return };
                self.input(format!("Ask {} in plain words", idx.name), "", move |a, q| {
                    if q.trim().is_empty() {
                        return;
                    }
                    let (st, id, q2) = (a.state.clone(), idx.id.clone(), q.clone());
                    a.status = "Searching…".into();
                    a.spawn(async move { st.rag_search(id, q2, 20).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                        Ok(res) => {
                            let (conn, db) = qurry_core::app::split_target(&idx.source);
                            let (conn, db) = (conn.to_string(), db.map(String::from));
                            a.open_console(&conn, db);
                            if let Some(Tab { kind: TabKind::Console(c), .. }) = a.tabs.get_mut(a.active) {
                                c.editor.insert_str(format!("-- AI Search ({}): {q}", idx.name));
                                c.result_title = format!("AI Search: {q}");
                                c.grid = Some(Grid::new(res));
                                c.show_output = false;
                            }
                            a.focus = Focus::Grid;
                            a.status = "Closest rows first (score 1 = same meaning)".into();
                        }
                        Err(e) => a.status = format!("AI Search: {e}"),
                    });
                });
            }
            Flow::RagBuild => {
                let Some(idx) = self.state.rag_indexes().ok().and_then(|l| l.into_iter().find(|x| x.id == value)) else { return };
                let label = format!("Embed {}", idx.name);
                let job = self.start_job(&label);
                let (st, id, j2) = (self.state.clone(), idx.id.clone(), job.clone());
                self.spawn(async move { st.rag_build(id, j2).await.map_err(|e| e.to_string()) }, move |a, r| {
                    a.end_job(&job);
                    a.status = match r {
                        Ok(n) => format!("Embedded {n} rows into {}", idx.collection),
                        Err(e) => format!("{label}: {e}"),
                    };
                });
            }
            Flow::RagDest(mut d) => {
                d.dest = value;
                let def = d.collection.clone();
                self.input("Table (pgvector) or collection to keep the vectors in", &def, move |a, c| {
                    d.collection = c.trim().to_string();
                    a.flow = Some(Flow::RagProvider(d));
                    a.popup = Some(palette(
                        "Embeddings made by",
                        vec![
                            PItem { label: "Ollama — runs on this computer".into(), hint: "free, nothing leaves this machine".into(), action: PAction::Pick("ollama".into()) },
                            PItem { label: "OpenAI — row text is sent to OpenAI".into(), hint: "needs an API key".into(), action: PAction::Pick("openai".into()) },
                        ],
                    ));
                });
            }
            Flow::RagProvider(mut d) => {
                d.provider = value;
                let def = default_model(&d.provider).to_string();
                self.input("Embedding model", &def, move |a, m| {
                    d.model = m.trim().to_string();
                    let openai = d.provider == "openai";
                    match a.state.rag_save(*d) {
                        Ok(x) => {
                            a.status = format!("Saved \"{}\" — ⌃K › Build RAG Index to embed the rows", x.name);
                            if openai && !a.state.rag_has_key() {
                                a.rag_key_start();
                            }
                        }
                        Err(e) => a.status = e.to_string(),
                    }
                });
            }
            _ => {}
        }
    }

    /// ⌃K › New RAG Index…: table picked in the tree → text column → where to store → collection → embedding (provider·model)
    pub fn rag_new_start(&mut self) {
        let Some(n) = self.tree.selected().filter(|n| matches!(&n.kind, NodeKind::Object(k) if k == "table" || k == "partitioned")) else {
            self.status = "Select a table in the explorer first".into();
            return;
        };
        let (conn, schema, table) = (n.conn.clone(), n.schema.clone().unwrap_or_default(), n.name.clone().unwrap_or_default());
        let kind = self.kind_of(&conn);
        if matches!(kind, DbKind::Redis | DbKind::Mongodb) || kind.is_vector() {
            self.status = "Pick a SQL table as the source".into();
            return;
        }
        let db = n.db.clone().unwrap_or_else(|| self.conn(&conn).map(|c| sql::default_db(&c.config)).unwrap_or_default());
        let source = sql::target_id(&conn, Some(&db));
        let stores: Vec<PItem> = self
            .conns
            .iter()
            .filter(|c| c.config.kind == DbKind::Postgres || c.config.kind.is_vector())
            .map(|c| PItem { label: c.config.name.clone(), hint: format!("{:?}", c.config.kind).to_lowercase().replace("postgres", "postgres (pgvector)"), action: PAction::Pick(c.config.id.clone()) })
            .collect();
        if stores.is_empty() {
            self.status = "Add a PostgreSQL (with pgvector), Qdrant, Chroma or Milvus data source to keep the vectors in".into();
            return;
        }
        let st = self.state.clone();
        let (s2, t2, src2) = (schema.clone(), table.clone(), source.clone());
        self.spawn(async move { st.table_detail(src2, s2, t2).await.map_err(|e| e.to_string()) }, move |a, r| {
            let d = match r {
                Ok(d) => d,
                Err(e) => {
                    a.status = e;
                    return;
                }
            };
            let Some(pk) = d.columns.iter().find(|c| c.is_pk).map(|c| c.name.clone()) else {
                a.status = format!("{schema}.{table} has no primary key — RAG needs a column that identifies each row");
                return;
            };
            let text: Vec<String> = d.columns.iter().filter(|c| !c.is_pk && ["char", "text", "string", "clob", "json"].iter().any(|t| c.data_type.to_lowercase().contains(t))).map(|c| c.name.clone()).collect();
            a.input(format!("Columns of {table} to embed (comma-separated)"), &text.join(", "), move |a, cols| {
                let text_columns: Vec<String> = cols.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                if text_columns.is_empty() {
                    return;
                }
                let d = RagIndex { name: format!("{table} search"), source, schema, table: table.clone(), id_column: pk, text_columns, collection: format!("{table}_embeddings"), ..Default::default() };
                a.flow = Some(Flow::RagDest(Box::new(d)));
                a.popup = Some(palette("Keep the vectors in", stores));
            });
        });
    }

    /// ⌃K › Set OpenAI API Key…: input is masked. Empty clears it
    pub fn rag_key_start(&mut self) {
        let mut t = TextArea::default();
        t.set_mask_char('•');
        t.set_cursor_line_style(ratatui::style::Style::default());
        self.popup = Some(Popup::Input {
            title: format!("OpenAI API key for RAG ({}; empty = remove)", if self.state.rag_has_key() { "saved" } else { "not set" }),
            input: t,
            on_ok: Box::new(|a, k| {
                let k = k.trim().to_string();
                a.status = match a.state.rag_set_key((!k.is_empty()).then_some(k.clone())) {
                    Ok(()) if k.is_empty() => "OpenAI API key removed".into(),
                    Ok(()) => "OpenAI API key saved (encrypted)".into(),
                    Err(e) => e.to_string(),
                };
            }),
            words: vec![],
            complete: None,
        });
    }
}
