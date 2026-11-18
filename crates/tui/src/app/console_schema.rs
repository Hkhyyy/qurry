//! A console's schema (#195), as the GUI's useTabs.openConsole and useConsole.switchSchema: a PostgreSQL console opened from a
//! schema (or an object in it) starts there, and Switch Schema… picks another (on MySQL, where a schema is a database, by USE)
use super::run::console_query;
use super::*;

impl App {
    /// The data source's Switch schema option: auto · manual · disable
    fn switch_schema_mode(&self, conn: &str) -> String {
        self.conn(conn).and_then(|c| c.config.options.switch_schema.clone()).unwrap_or_else(|| "auto".into())
    }

    /// A console just opened from `schema`: on PostgreSQL with Switch schema Auto its session moves there (else the default schema)
    pub(super) fn console_opened_in(&mut self, id: u64, schema: String) {
        let Some(conn) = self.tab_mut(id).map(|t| t.conn.clone()) else { return };
        if self.kind_of(&conn) != DbKind::Postgres || self.switch_schema_mode(&conn) != "auto" {
            return;
        }
        if let Some(TabKind::Console(c)) = self.tab_mut(id).map(|t| &mut t.kind) {
            c.schema = Some(schema.clone());
        }
        let how = format!("the console is in the default schema ({} › Switch Schema… picks one)", self.keys.label(Action::Palette));
        self.run_schema_switch(id, schema, move |a, r| {
            if let Err(e) = r {
                if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    c.schema = None;
                }
                a.status = format!("{e} — {how}");
            }
        });
    }

    /// Switch Schema…: the console's schemas (system ones left out), the current one marked
    pub fn switch_schema_start(&mut self) {
        let Some(t) = self.tabs.get(self.active).filter(|t| matches!(t.kind, TabKind::Console(_))) else {
            self.status = "Switch Schema… is for a query console".into();
            return;
        };
        let (id, conn, target) = (t.id, t.conn.clone(), t.target.clone());
        if !matches!(self.kind_of(&conn), DbKind::Postgres | DbKind::Mysql) {
            self.status = "Switch Schema… is for PostgreSQL and MySQL consoles".into();
            return;
        }
        if self.switch_schema_mode(&conn) == "disable" {
            self.status = "Schema switching is disabled for this data source".into();
            return;
        }
        let st = self.state.clone();
        self.status = "Reading schemas…".into();
        self.spawn(async move { st.list_schemas(target).await.map_err(err) }, move |a, r| match r {
            Ok(v) => a.console_schema_choices(id, v),
            Err(e) => a.status = e,
        });
    }

    /// Switch Schema…'s list: the current schema (the default one: public, or the database on MySQL) selected
    pub(crate) fn console_schema_choices(&mut self, id: u64, schemas: Vec<qurry_core::meta::SchemaInfo>) {
        let Some(t) = self.tab_index(id).map(|i| &self.tabs[i]) else { return };
        let TabKind::Console(c) = &t.kind else { return };
        let cur = match &c.schema {
            Some(s) => s.clone(),
            None if self.kind_of(&t.conn) == DbKind::Mysql => t.db.clone(),
            None => "public".into(),
        };
        let names: Vec<String> = schemas.into_iter().filter(|s| !s.is_system).map(|s| s.name).collect();
        let sel = names.iter().position(|n| *n == cur).unwrap_or(0);
        let items = names.into_iter().map(|n| PItem { hint: if n == cur { "current".into() } else { String::new() }, action: PAction::Pick(n.clone()), label: n }).collect();
        self.status.clear();
        self.flow = Some(crate::migrate::Flow::ConsoleSchema(id));
        self.open_after_notices(Popup::Palette(Palette { title: "Switch schema".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    /// Picked in Switch Schema…
    pub(crate) fn switch_console_schema(&mut self, id: u64, schema: String) {
        let mysql = self.tab_mut(id).map(|t| t.conn.clone()).is_some_and(|c| self.kind_of(&c) == DbKind::Mysql);
        let picked = schema.clone();
        self.run_schema_switch(id, schema, move |a, r| match r {
            Ok(()) => {
                if let Some(TabKind::Console(c)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    c.schema = Some(picked.clone());
                }
                a.status = if mysql { format!("USE {picked}") } else { format!("search_path = {picked}, public") };
            }
            Err(e) => a.status = e,
        });
    }

    /// Sends the switch (search_path, or USE on MySQL) on the console's session, through the normal run path
    fn run_schema_switch(&mut self, id: u64, schema: String, done: impl FnOnce(&mut App, Result<(), String>) + Send + 'static) {
        let Some(t) = self.tab_mut(id) else { return };
        let TabKind::Console(c) = &t.kind else { return };
        let (target, session, conn) = (t.target.clone(), c.session.clone(), t.conn.clone());
        let q = if self.kind_of(&conn) == DbKind::Mysql { qurry_shared::sql::use_sql(&schema) } else { qurry_shared::sql::search_path_sql(&schema) };
        let (st, fake) = (self.state.clone(), self.fake_db());
        self.spawn(async move { console_query(&st, fake, &target, q, None, &session).await.map(|_| ()) }, done);
    }
}
