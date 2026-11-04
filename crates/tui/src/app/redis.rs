use super::*;

/// Redis value → grid (None for string: shown as text)
pub fn redis_grid(d: &KeyDetail) -> Option<Grid> {
    let arr = d.value.as_array()?;
    let (names, rows): (Vec<&str>, Vec<Vec<Value>>) = match d.ty.as_str() {
        "hash" => (vec!["field", "value"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "zset" => (vec!["member", "score"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "list" => (vec!["index", "value"], arr.iter().enumerate().map(|(i, x)| vec![Value::from(i), x.clone()]).collect()),
        "set" => (vec!["member"], arr.iter().map(|x| vec![x.clone()]).collect()),
        "stream" => (vec!["id", "fields"], arr.iter().map(|x| vec![x[0].clone(), Value::from(x[1].to_string())]).collect()),
        _ => return None,
    };
    let result = QueryResult {
        columns: names.iter().map(|n| ColumnInfo { name: n.to_string(), type_name: "TEXT".into() }).collect(),
        rows,
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 0,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
    };
    Some(Grid::new(result))
}

impl App {
    pub fn open_redis(&mut self, conn: &str, db: String, key: Option<String>) {
        let id = match self.tabs.iter().position(|t| t.conn == conn && t.db == db && matches!(t.kind, TabKind::Redis(_))) {
            Some(i) => {
                self.activate(i);
                self.tabs[i].id
            }
            None => {
                let name = self.conn(conn).map(|c| c.config.name.clone()).unwrap_or_default();
                let id = self.push_tab(conn, db.clone(), format!("{name} [{db}]"), TabKind::Redis(RedisTab { pattern: "*".into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: HashSet::new(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: key.clone() }));
                self.redis_scan(id, true);
                id
            }
        };
        if let Some(k) = key {
            if let Some(t) = self.tab_mut(id) {
                if let TabKind::Redis(r) = &mut t.kind {
                    if !r.reveal(&k) {
                        r.want = Some(k.clone());
                    }
                }
            }
            self.redis_load_key(id, k);
        }
    }

    pub fn redis_scan(&mut self, id: u64, reset: bool) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Redis(r) = &mut t.kind else { return };
        r.scanning = true;
        let (cursor, pattern) = (if reset { "0".to_string() } else { r.cursor.clone() }, r.pattern.clone());
        let st = self.state.clone();
        self.spawn(async move { st.redis_scan(target, cursor, pattern, 500, None).await.map_err(err) }, move |a, res| {
            let Some(t) = a.tab_mut(id) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            r.scanning = false;
            match res {
                Ok(page) => {
                    if reset {
                        r.keys.clear();
                        r.sel = 0;
                        r.off = 0;
                    }
                    let mut seen: HashSet<String> = r.keys.iter().map(|k| k.key.clone()).collect();
                    for k in page.keys {
                        if seen.insert(k.key.clone()) {
                            r.keys.push(k);
                        }
                    }
                    r.keys.sort_by(|a, b| a.key.cmp(&b.key));
                    r.sel = r.sel.min(r.rows().len().saturating_sub(1));
                    r.cursor = page.cursor;
                    let n = r.keys.len();
                    let more = r.cursor != "0";
                    let first = match r.want.take() {
                        Some(w) => {
                            r.reveal(&w);
                            None
                        }
                        // If the first row is a key, show it right away (if a folder, expand it to pick)
                        None if reset && r.detail.is_none() => r.sel_key(),
                        None => None,
                    };
                    a.status = format!("{n}{} keys", if more { "+ (m: scan more)" } else { "" });
                    if let Some(k) = first {
                        a.redis_load_key(id, k);
                    }
                }
                Err(e) => a.status = e,
            }
        });
    }

    pub fn redis_load_key(&mut self, id: u64, key: String) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let st = self.state.clone();
        let k2 = key.clone();
        self.spawn(async move { st.redis_key(target, k2, Some(1000)).await.map_err(err) }, move |a, res| {
            let Some(t) = a.tab_mut(id) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            match res {
                Ok(d) => {
                    r.value = redis_grid(&d);
                    r.text = d.value.as_str().map(|s| match serde_json::from_str::<Value>(s) {
                        Ok(v) if v.is_object() || v.is_array() => serde_json::to_string_pretty(&v).unwrap_or_default(),
                        _ => s.to_string(),
                    });
                    r.detail = Some((key, d));
                }
                Err(e) => {
                    r.detail = None;
                    r.value = None;
                    a.status = e;
                }
            }
        });
    }

    /// Re-read keys after running a Redis command
    pub fn redis_cmd(&mut self, id: u64, args: Vec<String>, rescan: bool) {
        if self.tab_mut(id).is_none() {
            return;
        }
        if self.conn(&self.tabs[self.tab_index(id).unwrap()].conn).map(|c| c.config.read_only).unwrap_or(false) {
            self.status = "Data source is read-only".into();
            return;
        }
        let t = self.tab_mut(id).unwrap();
        let target = t.target.clone();
        let key = match &t.kind {
            TabKind::Redis(r) => r.detail.as_ref().map(|d| d.0.clone()),
            _ => None,
        };
        let st = self.state.clone();
        let shown = args.join(" ");
        self.spawn(async move { st.redis_exec(target, args).await.map_err(err) }, move |a, res| match res {
            Ok(v) => {
                a.status = format!("{shown} → {}", crate::ui::format_reply(&v).lines().next().unwrap_or(""));
                if rescan {
                    a.redis_scan(id, true);
                }
                if let Some(k) = key {
                    a.redis_load_key(id, k);
                }
            }
            Err(e) => a.status = e,
        });
    }
}
