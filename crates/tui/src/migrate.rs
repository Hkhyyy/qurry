//! 비교·마이그레이션 (⌃K): Compare Schemas… (원본 → 대상 스키마 고르기 → DDL 을 대상 콘솔에), Compare Table Data… (같은 이름 테이블 → 동기화 SQL),
//! Run Migrations… (폴더 → 상태 → 확인하고 적용)
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind};
use crate::tree::NodeKind;
use qurry_core::sql;
use tui_textarea::TextArea;

/// 고르는 중인 단계
#[derive(Clone, Debug)]
pub enum Flow {
    /// 원본 스키마 고르기
    SchemaSource,
    /// 대상 스키마 고르기 (원본: conn, db, schema)
    SchemaTarget(String, String, String),
    /// 같은 이름 테이블이 있을 대상 스키마 고르기 (원본: conn, db, schema, table)
    DataTarget(String, String, String, String),
    /// RAG: 검색할 · 만들 인덱스 고르기, 새 인덱스의 저장할 곳 · 임베딩 제공자 고르기 (crate::rag)
    RagSearch,
    RagBuild,
    RagDest(Box<qurry_core::rag::RagIndex>),
    RagProvider(Box<qurry_core::rag::RagIndex>),
}

/// 고를 수 있는 곳 하나: "conn\u{1}db\u{1}schema"
fn pack(conn: &str, db: &str, schema: &str) -> String {
    format!("{conn}\u{1}{db}\u{1}{schema}")
}

fn unpack(v: &str) -> (String, String, String) {
    let mut p = v.split('\u{1}').map(String::from);
    (p.next().unwrap_or_default(), p.next().unwrap_or_default(), p.next().unwrap_or_default())
}

impl App {
    /// 접속한 SQL Data Source 들의 (기본 DB 의) 스키마로 고르기 창을 띄운다
    fn pick_schema(&mut self, title: &str, flow: Flow) {
        let conns: Vec<(String, String, String, qurry_core::store::DbKind)> = self
            .conns
            .iter()
            .filter(|c| c.connected && c.config.kind != qurry_core::store::DbKind::Redis)
            .map(|c| (c.config.id.clone(), c.config.name.clone(), sql::default_db(&c.config), c.config.kind))
            .collect();
        if conns.is_empty() {
            self.status = "Connect to a PostgreSQL or MySQL data source first".into();
            return;
        }
        let st = self.state.clone();
        let title = title.to_string();
        self.status = "Reading schemas…".into();
        self.spawn(
            async move {
                let mut items = vec![];
                for (id, name, db, kind) in conns {
                    let target = sql::target_id(&id, Some(&db));
                    let schemas: Vec<String> = match kind {
                        // MySQL: DB 가 곧 스키마
                        qurry_core::store::DbKind::Mysql => st.list_databases(id.clone()).await.unwrap_or_default(),
                        _ => st.list_schemas(target).await.map(|v| v.into_iter().filter(|s| !s.is_system).map(|s| s.name).collect()).unwrap_or_default(),
                    };
                    for s in schemas {
                        let db = if kind == qurry_core::store::DbKind::Mysql { s.clone() } else { db.clone() };
                        items.push(PItem { label: format!("{name} / {db} / {s}"), hint: String::new(), action: PAction::Pick(pack(&id, &db, &s)) });
                    }
                }
                items
            },
            move |a, items| {
                a.status.clear();
                a.flow = Some(flow);
                a.popup = Some(Popup::Palette(Palette { title, input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
            },
        );
    }

    /// ⌃K › Compare Schemas…
    pub fn compare_schemas_start(&mut self) {
        self.pick_schema("Compare Schemas — source (the one to copy from)", Flow::SchemaSource);
    }

    /// ⌃K › Compare Table Data…: 트리에서 고른 테이블, 아니면 지금 테이블 탭
    pub fn compare_data_start(&mut self) {
        let src = if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected().filter(|n| matches!(&n.kind, NodeKind::Object(k) if k == "table" || k == "partitioned"))
        {
            Some((n.conn.clone(), n.db.clone().unwrap_or_default(), n.schema.clone().unwrap_or_default(), n.name.clone().unwrap_or_default()))
        } else if let Some(Tab { kind: TabKind::Table(tt), conn, db, .. }) = self.tabs.get(self.active) {
            Some((conn.clone(), db.clone(), tt.schema.clone(), tt.table.clone()))
        } else {
            None
        };
        let Some((c, d, s, t)) = src else {
            self.status = "Select a table (or open one) to compare its data".into();
            return;
        };
        self.pick_schema(&format!("Compare Data of {s}.{t} — target schema (a table with the same name)"), Flow::DataTarget(c, d, s, t));
    }

    /// 고르기 창에서 고른 것
    pub fn flow_pick(&mut self, value: String) {
        let (conn, db, schema) = unpack(&value);
        match self.flow.take() {
            Some(f @ (Flow::RagSearch | Flow::RagBuild | Flow::RagDest(_) | Flow::RagProvider(_))) => self.rag_flow(f, value),
            Some(Flow::SchemaSource) => self.pick_schema(&format!("Compare Schemas — target (made to look like {schema})"), Flow::SchemaTarget(conn, db, schema)),
            Some(Flow::SchemaTarget(sc, sd, ss)) => {
                let st = self.state.clone();
                let (src, dst) = (sql::target_id(&sc, Some(&sd)), sql::target_id(&conn, Some(&db)));
                let (ss2, ds) = (ss.clone(), schema.clone());
                self.status = "Comparing…".into();
                self.spawn(async move { st.compare_schemas(src, ss2, dst, ds).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                    Ok(changes) if changes.is_empty() => a.status = format!("{ss} and {schema} match — nothing to change"),
                    Ok(changes) => {
                        let n = changes.len();
                        let mut sql = format!("-- Make {schema} look like {ss} ({n} change(s)). Review, then run (F5).\n");
                        let mut drops = String::new();
                        for c in &changes {
                            if c.destructive {
                                drops += &format!("-- {}: {}\n{}\n", c.kind, c.object, c.sql.lines().map(|l| format!("-- {l}")).collect::<Vec<_>>().join("\n"));
                            } else {
                                sql += &format!("\n-- {}: {}\n{}\n", c.kind, c.object, c.sql);
                            }
                        }
                        if !drops.is_empty() {
                            sql += &format!("\n-- ── Drops (data is lost) — uncomment to run ──\n{drops}");
                        }
                        a.open_console(&conn, Some(db));
                        if let Some(Tab { kind: TabKind::Console(c), .. }) = a.tabs.get_mut(a.active) {
                            c.editor.insert_str(sql);
                        }
                        a.status = format!("{n} change(s) — the SQL is in a new console on the target");
                    }
                    Err(e) => a.status = format!("Compare: {e}"),
                });
            }
            Some(Flow::DataTarget(sc, sd, ss, st_)) => {
                let st = self.state.clone();
                let (src, dst) = (sql::target_id(&sc, Some(&sd)), sql::target_id(&conn, Some(&db)));
                let (ss2, ds, t1, t2) = (ss.clone(), schema.clone(), st_.clone(), st_.clone());
                self.status = "Comparing rows…".into();
                self.spawn(async move { st.compare_data(src, ss2, t1, dst, ds, t2).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                    Ok(d) => {
                        let summary = format!("{} same · {} different · {} only in source · {} only in target{}", d.same, d.different, d.only_source, d.only_target, if d.truncated { " (first 200,000 rows)" } else { "" });
                        if !d.sql.is_empty() {
                            a.open_console(&conn, Some(db));
                            if let Some(Tab { kind: TabKind::Console(c), .. }) = a.tabs.get_mut(a.active) {
                                c.editor.insert_str(format!("-- {ss}.{st_} → {schema}.{st_}: {summary}\n\n{}", d.sql));
                            }
                        }
                        a.status = summary;
                    }
                    Err(e) => a.status = format!("Compare data: {e}"),
                });
            }
            None => {}
        }
    }

    /// ⌃K › Run Migrations…: 폴더 → 상태 → 적용할 것이 있으면 묻는다
    pub fn migrations_start(&mut self) {
        let target = match self.tabs.get(self.active) {
            Some(t) if self.focus != Focus::Tree => Some((t.conn.clone(), t.db.clone())),
            _ => self.tree.selected().map(|n| (n.conn.clone(), n.db.clone().unwrap_or_else(|| self.conn(&n.conn).map(|c| sql::default_db(&c.config)).unwrap_or_default()))),
        };
        let Some((conn, db)) = target else {
            self.status = "Select a data source or database first".into();
            return;
        };
        let home = std::env::var("HOME").unwrap_or_default();
        self.input("Migrations folder (V1__name.sql, V2__…)", &format!("{home}/"), move |a, dir| {
            let dir = match dir.trim().strip_prefix("~/") {
                Some(r) => format!("{home}/{r}"),
                None => dir.trim().to_string(),
            };
            let st = a.state.clone();
            let target = sql::target_id(&conn, Some(&db));
            let (t2, d2) = (target.clone(), dir.clone());
            a.spawn(async move { st.migration_status(t2, d2).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                Ok(list) => {
                    let lines: Vec<String> = list.iter().map(|m| format!("  V{:<6} {:<9} {}", m.version, m.status, m.description)).collect();
                    let pending = list.iter().filter(|m| m.status == "pending").count();
                    let text = format!("{}\n\n{}", if lines.is_empty() { "No V*__*.sql files in this folder".into() } else { lines.join("\n") }, if pending > 0 { format!("Apply {pending} pending migration(s)?") } else { "Nothing to apply.".into() });
                    if pending == 0 {
                        a.popup = Some(Popup::Message { title: "Migrations".into(), text });
                        return;
                    }
                    let st = a.state.clone();
                    a.popup = Some(Popup::Confirm {
                        text,
                        on_yes: Box::new(move |a| {
                            a.status = "Applying migrations…".into();
                            a.spawn(async move { st.apply_migrations(target, dir).await.map_err(|e| e.to_string()) }, |a, r| {
                                a.status = match r {
                                    Ok(v) => format!("Applied {} migration(s): V{}", v.len(), v.join(", V")),
                                    Err(e) => e,
                                }
                            });
                        }),
                    });
                }
                Err(e) => a.status = format!("Migrations: {e}"),
            });
        });
    }
}
