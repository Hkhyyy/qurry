use super::*;

impl App {
    // ───────── Data Sources ─────────

    pub fn reload_conns(&mut self) {
        let st = self.state.clone();
        self.spawn(async move { st.list_connections().await.map_err(err) }, |a, r| match r {
            Ok(cs) => {
                // Data Source just connected: reload table tabs left empty by the failed connection
                let back: Vec<String> = cs.iter().filter(|c| c.connected && a.conn(&c.config.id).is_some_and(|o| !o.connected)).map(|c| c.config.id.clone()).collect();
                a.conns = cs;
                let first = !a.restored;
                if first {
                    a.restored = true;
                    if let Some(v) = a.state.store.workspace("tui") {
                        crate::workspace::restore(a, &v);
                    }
                    a.ws_adopt_shared();
                }
                a.ws_adopt_orphans();
                a.ws_push();
                a.rebuild_roots();
                if first {
                    a.saved_ws = crate::workspace::to_json(a).to_string();
                }
                for c in back {
                    a.retry_failed_tables(&c);
                }
            }
            Err(e) => a.status = format!("Failed to load data sources: {e}"),
        });
    }

    /// Top of the explorer: only the current workspace's Data Sources (in the order added to the workspace)
    pub fn rebuild_roots(&mut self) {
        let mine = &self.workspaces[self.cur_ws].conns;
        self.tree.roots = mine
            .iter()
            .filter_map(|id| self.conns.iter().find(|c| &c.config.id == id))
            .map(|c| Node {
                key: c.config.id.clone(),
                label: c.config.name.clone(),
                hint: format!("{:?}", c.config.kind).to_lowercase() + if c.config.active_ssh().is_some() { " · ssh" } else if c.config.active_ssm().is_some() { " · ssm" } else { "" },
                kind: NodeKind::Conn,
                conn: c.config.id.clone(),
                db: None,
                schema: None,
                name: None,
                expandable: true,
            })
            .collect();
        self.tree.clamp();
    }

    /// Data Sources in no workspace (just created, or created in the GUI) → the current workspace
    pub fn ws_adopt_orphans(&mut self) {
        let known: HashSet<String> = self.workspaces.iter().flat_map(|w| w.conns.iter().cloned()).collect();
        let orphans: Vec<String> = self.conns.iter().map(|c| c.config.id.clone()).filter(|id| !known.contains(id)).collect();
        let cur = self.cur_ws;
        self.workspaces[cur].conns.extend(orphans);
    }

    pub(crate) fn default_db(&self, conn: &str) -> String {
        self.conn(conn).map(|c| sql::default_db(&c.config)).unwrap_or_default()
    }

    // ───────── Tree ─────────

    pub fn toggle_node(&mut self, n: &Node) {
        if !n.expandable {
            return;
        }
        if self.tree.expanded.contains(&n.key) {
            self.tree.expanded.remove(&n.key);
        } else {
            self.tree.expanded.insert(n.key.clone());
            // An error from the last load isn't cached: expanding again tries again (and asks about TLS again)
            if self.tree.failed(&n.key) {
                self.tree.children.remove(&n.key);
            }
            self.load_children(n.clone());
            // If the connection fell back to plaintext, say so in the status bar (same wording as the GUI)
            if matches!(n.kind, NodeKind::Conn) && matches!(self.kind_of(&n.conn), DbKind::Postgres | DbKind::Mysql) {
                let (st, id) = (self.state.clone(), n.conn.clone());
                self.spawn(async move { st.connect(id).await.map_err(|e| e.to_string()) }, |a, r| {
                    if let Ok(v) = r
                        && let Some((_, note)) = v.split_once(" — ⚠")
                    {
                        a.status = format!("⚠{note}");
                    }
                });
            }
        }
        self.tree.clamp();
    }

    /// Load a node's children again (after the driver was installed or unencrypted was allowed)
    pub(crate) fn reexpand(&mut self, key: &str) {
        self.tree.children.remove(key);
        self.tree.expanded.remove(key);
        if let Some(n) = self.tree.visible().into_iter().map(|(_, n)| n).find(|n| n.key == key).cloned() {
            self.toggle_node(&n);
        }
    }

    pub fn load_children(&mut self, n: Node) {
        let key = n.key.clone();
        if self.tree.children.contains_key(&key) || self.tree.loading.contains(&key) {
            return;
        }
        self.tree.loading.insert(key.clone());
        let st = self.state.clone();
        let conn = n.conn.clone();
        let kind = self.kind_of(&conn);
        let cfg = self.conn(&conn).map(|c| c.config.clone());
        let conn_id = conn.clone();
        let done = move |a: &mut App, key: String, r: Result<Vec<Node>, String>| {
            a.tree.loading.remove(&key);
            match r {
                Ok(nodes) => {
                    a.tree.children.insert(key, nodes);
                }
                Err(e) => {
                    a.tree.children.insert(key.clone(), vec![Node::info(&key, "", format!("⚠ {e}"))]);
                    // If the driver is missing, ask whether to download it (expand again afterwards)
                    if e.contains(qurry_core::plugin::MISSING) && let Some(d) = kind.driver() {
                        let k2 = key.clone();
                        a.open_after_notices(Popup::Confirm {
                            opened: None,
                            text: format!("The {d} driver isn't installed.\n\nDownload it from this Qurry version's GitHub release (its signature is checked) and connect?"),
                            on_yes: Box::new(move |a| {
                                a.status = format!("Installing the {d} driver…");
                                a.spawn(async move { qurry_core::plugin::install(d).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                                    Ok(_) => {
                                        a.status = format!("{d} driver installed");
                                        a.reexpand(&k2);
                                    }
                                    Err(e) => a.status = e,
                                });
                            }),
                        });
                    }
                    a.status = if e.contains(qurry_core::tunnel::UNKNOWN_HOST) { format!("{e} — ⌃k › Trust SSH Host Key") } else { e.clone() };
                    a.offer_tls_opt_in(&conn_id, &e, move |a| a.reexpand(&key));
                }
            }
            a.tree.clamp();
        };
        match n.kind {
            NodeKind::Conn => {
                let cfg = cfg.unwrap();
                let def = sql::default_db(&cfg);
                self.spawn(
                    async move {
                        let dbs = st.list_databases(conn.clone()).await.map_err(err)?;
                        // Connected: fill the search cache in the background so lookups work next time without connecting
                        let st2 = st.clone();
                        tokio::spawn(async move { st2.warm_search_cache(conn).await });
                        Ok::<_, String>((dbs, def))
                    },
                    move |a, r: Result<(Vec<String>, String), String>| {
                        let r = r.map(|(dbs, def)| {
                            qurry_core::app::visible_dbs(&cfg, dbs, &def)
                                .into_iter()
                                .map(|d| Node {
                                    key: format!("{}|{d}", cfg.id),
                                    label: d.clone(),
                                    hint: if d == def { "default".into() } else { String::new() },
                                    kind: NodeKind::Db,
                                    conn: cfg.id.clone(),
                                    db: Some(d),
                                    schema: None,
                                    name: None,
                                    expandable: true,
                                })
                                .collect::<Vec<_>>()
                        });
                        // DB to auto-expand: the only visible DB if there's one, otherwise the default DB (only if visible)
                        let auto = r.as_ref().ok().and_then(|ns| if ns.len() == 1 { ns.first().cloned() } else { ns.iter().find(|n| n.hint == "default").cloned() });
                        done(a, key, r);
                        a.reload_conns();
                        if let Some(n) = auto {
                            a.tree.expanded.insert(n.key.clone());
                            a.load_children(n);
                        }
                    },
                );
            }
            NodeKind::Db => {
                let db = n.db.clone().unwrap();
                let target = sql::target_id(&conn, Some(&db));
                let flat = kind.flat();
                self.spawn(
                    async move {
                        let schemas = st.list_schemas(target.clone()).await.map_err(err)?;
                        if flat {
                            let s = schemas.first().map(|s| s.name.clone()).unwrap_or_else(|| db.clone());
                            let objs = st.list_objects(target, s.clone()).await.map_err(err)?;
                            Ok::<_, String>((schemas, Some((s, objs))))
                        } else {
                            Ok((schemas, None))
                        }
                    },
                    move |a, r| match r {
                        Err(e) => done(a, key, Err(e)),
                        Ok((_, Some((schema, objs)))) => {
                            let groups = a.groups(&key, &n.conn, n.db.as_deref().unwrap(), &schema, objs);
                            done(a, key, Ok(groups));
                        }
                        Ok((schemas, None)) => {
                            let scope = a.conn(&n.conn).map(|c| c.config.scope.clone()).unwrap_or_default();
                            let db = n.db.clone().unwrap_or_default();
                            let user: Vec<_> = schemas.iter().filter(|s| scope.schema_visible(&db, &s.name, s.is_system, false)).collect();
                            let nodes: Vec<Node> = user
                                .iter()
                                .map(|s| Node {
                                    key: format!("{key}|{}", s.name),
                                    label: s.name.clone(),
                                    hint: String::new(),
                                    kind: NodeKind::Schema,
                                    conn: n.conn.clone(),
                                    db: n.db.clone(),
                                    schema: Some(s.name.clone()),
                                    name: None,
                                    expandable: true,
                                })
                                .collect();
                            let auto = if nodes.len() == 1 { nodes.first().cloned() } else { nodes.iter().find(|x| x.label == "public").cloned() };
                            done(a, key, Ok(nodes));
                            if let Some(s) = auto {
                                a.tree.expanded.insert(s.key.clone());
                                a.load_children(s);
                            }
                        }
                    },
                );
            }
            NodeKind::Schema => {
                let (db, schema) = (n.db.clone().unwrap(), n.schema.clone().unwrap());
                let target = sql::target_id(&conn, Some(&db));
                let s2 = schema.clone();
                self.spawn(async move { st.list_objects(target, s2).await.map_err(err) }, move |a, r| {
                    let r = r.map(|objs| a.groups(&key, &n.conn, &db, &schema, objs));
                    done(a, key, r);
                });
            }
            NodeKind::Object(_) if n.is_relation() => {
                let target = sql::target_id(&conn, n.db.as_deref());
                let (schema, name) = (n.schema.clone().unwrap(), n.name.clone().unwrap());
                self.spawn(async move { st.table_detail(target, schema, name).await.map_err(err) }, move |a, r| {
                    let r = r.map(|d| {
                        d.columns
                            .into_iter()
                            .map(|c| Node {
                                key: format!("{key}|c:{}", c.name),
                                label: c.name.clone(),
                                hint: format!("{}{}", c.data_type, if c.nullable { "" } else { " not null" }),
                                kind: NodeKind::Column { ty: c.data_type, pk: c.is_pk },
                                conn: n.conn.clone(),
                                db: n.db.clone(),
                                schema: n.schema.clone(),
                                name: Some(c.name),
                                expandable: false,
                            })
                            .collect()
                    });
                    done(a, key, r);
                });
            }
            _ => {
                self.tree.loading.remove(&key);
            }
        }
    }

    /// Schema objects → folders (tables/views/…/keys) + nodes under them. The first folder is expanded
    fn groups(&mut self, parent: &str, conn: &str, db: &str, schema: &str, objs: SchemaObjects) -> Vec<Node> {
        let defs: [(&str, &[&str]); 5] = [
            ("tables", &["table", "partitioned", "foreign"]),
            ("views", &["view"]),
            ("materialized views", &["matview"]),
            ("sequences", &["sequence"]),
            ("keys", &["key"]),
        ];
        let mut out = vec![];
        let mk_obj = |gkey: &str, name: &str, kind: &str, hint: String| Node {
            key: format!("{gkey}|o:{name}"),
            label: name.to_string(),
            hint,
            kind: NodeKind::Object(kind.to_string()),
            conn: conn.into(),
            db: Some(db.into()),
            schema: Some(schema.into()),
            name: Some(name.into()),
            expandable: ["table", "view", "matview", "foreign", "partitioned"].contains(&kind),
        };
        for (label, kinds) in defs {
            let items: Vec<_> = objs.relations.iter().filter(|r| kinds.contains(&r.kind.as_str())).collect();
            if items.is_empty() {
                continue;
            }
            let gkey = format!("{parent}|g:{label}");
            let kids = items
                .iter()
                .map(|r| mk_obj(&gkey, &r.name, if r.kind == "key" { "key" } else { &r.kind }, if r.kind == "key" { r.comment.clone().unwrap_or_default() } else { r.comment.clone().unwrap_or_default() }))
                .collect();
            // Redis keys are grouped into folders by ":"
            let kids = if label == "keys" { crate::tree::key_folders(&mut self.tree.children, &gkey, kids) } else { kids };
            self.tree.children.insert(gkey.clone(), kids);
            out.push(Node { key: gkey, label: label.into(), hint: items.len().to_string(), kind: NodeKind::Group, conn: conn.into(), db: Some(db.into()), schema: Some(schema.into()), name: None, expandable: true });
        }
        if !objs.routines.is_empty() {
            let gkey = format!("{parent}|g:routines");
            let kids = objs
                .routines
                .iter()
                .map(|r| {
                    let mut n = mk_obj(&gkey, &r.name, &r.kind, format!("({}){}", r.args, r.returns.as_ref().map(|x| format!(" → {x}")).unwrap_or_default()));
                    n.key = format!("{gkey}|o:{}({})", r.name, r.args);
                    n
                })
                .collect();
            self.tree.children.insert(gkey.clone(), kids);
            out.push(Node { key: gkey, label: "routines".into(), hint: objs.routines.len().to_string(), kind: NodeKind::Group, conn: conn.into(), db: Some(db.into()), schema: Some(schema.into()), name: None, expandable: true });
        }
        if let Some(first) = out.first() {
            self.tree.expanded.insert(first.key.clone());
        }
        out
    }

    /// Enter / F4 / double-click
    pub fn activate_node(&mut self, n: &Node, f4: bool) {
        let kind = self.kind_of(&n.conn);
        // Connection·DB·schema·folder: Enter/double-click = expand (regardless of DB kind). F4 / Ctrl+↓ = console, key browser for Redis
        match &n.kind {
            NodeKind::Conn | NodeKind::Db if f4 && kind == DbKind::Redis => {
                let db = n.db.clone().unwrap_or_else(|| self.default_db(&n.conn));
                self.open_redis(&n.conn, db, None)
            }
            NodeKind::Conn | NodeKind::Db if f4 => self.open_console(&n.conn, n.db.clone()),
            NodeKind::Object(k) if k == "key" => self.open_redis(&n.conn, n.db.clone().unwrap(), n.name.clone()),
            NodeKind::Object(k) if n.is_relation() => self.open_table(&n.conn, n.db.clone().unwrap(), n.schema.clone().unwrap(), n.name.clone().unwrap(), k.clone()),
            NodeKind::Object(k) => {
                let k = if k == "function" || k == "procedure" { "routine".to_string() } else { k.clone() };
                self.open_ddl(&n.conn, n.db.clone().unwrap(), n.schema.clone().unwrap(), n.name.clone().unwrap(), k)
            }
            NodeKind::Column { .. } => {}
            _ => self.toggle_node(n),
        }
    }
}
