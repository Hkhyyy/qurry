//! Manage Shown Schemas: window for choosing which DBs and schemas show in the tree (same rules as GUI src/components/ScopePopup.tsx).
//! Only state and check rules live here; drawing is in ui.rs, keys in input.rs
use qurry_core::meta::SchemaInfo;
use qurry_core::store::{Scope, SCOPE_ALL, SCOPE_DEFAULT};
use std::collections::{HashMap, HashSet};

/// One line of the window
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    AllDbs,
    DefaultDb,
    Db(String),
    AllSchemas(String),
    DefaultSchema(String),
    Schema { db: String, name: String, system: bool },
    /// Schemas loading or failed (db, text)
    Note(String, String),
}

/// Check mark
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Check {
    Off,
    On,
    /// Shown only by the All databases / Default database rules
    Implicit,
}

pub struct ScopeEditor {
    pub conn: String,
    pub dbs: Vec<String>,
    pub default_db: String,
    /// DBs to show ("*" = all, "@default" = default DB)
    databases: Vec<String>,
    /// DB → schemas to show (none = all)
    schemas: HashMap<String, Vec<String>>,
    open: HashSet<String>,
    /// Loaded schemas (none = loading)
    loaded: HashMap<String, Result<Vec<SchemaInfo>, String>>,
    pub sel: usize,
}

impl ScopeEditor {
    /// focus_db: if opened from a DB line, start with that DB expanded and selected (schemas are loaded and passed in via set_schemas)
    pub fn new(conn: String, dbs: Vec<String>, default_db: String, scope: &Scope, focus_db: Option<&str>) -> Self {
        let databases = match &scope.databases {
            Some(d) if !d.is_empty() => d.clone(),
            _ => vec![SCOPE_ALL.into()],
        };
        let mut e = ScopeEditor { conn, dbs, default_db, databases, schemas: scope.schemas.clone(), open: HashSet::new(), loaded: HashMap::new(), sel: 0 };
        if let Some(db) = focus_db {
            e.open.insert(db.to_string());
            e.sel = e.rows().iter().position(|r| *r == Row::Db(db.to_string())).unwrap_or(0);
        }
        e
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut out = vec![Row::AllDbs, Row::DefaultDb];
        for db in &self.dbs {
            out.push(Row::Db(db.clone()));
            if !self.open.contains(db) {
                continue;
            }
            match self.loaded.get(db) {
                None => out.push(Row::Note(db.clone(), "Loading…".into())),
                Some(Err(e)) => out.push(Row::Note(db.clone(), e.clone())),
                Some(Ok(list)) => {
                    out.push(Row::AllSchemas(db.clone()));
                    out.push(Row::DefaultSchema(db.clone()));
                    // System schemas first, then by name (same as the GUI)
                    let mut list: Vec<&SchemaInfo> = list.iter().collect();
                    list.sort_by(|a, b| b.is_system.cmp(&a.is_system).then_with(|| a.name.cmp(&b.name)));
                    out.extend(list.into_iter().map(|s| Row::Schema { db: db.clone(), name: s.name.clone(), system: s.is_system }));
                }
            }
        }
        out
    }

    /// Schemas to show for this DB (all if there is no filter)
    fn sch_list(&self, db: &str) -> Vec<String> {
        self.schemas.get(db).cloned().unwrap_or_else(|| vec![SCOPE_ALL.into()])
    }

    pub fn check(&self, row: &Row) -> Check {
        let on = |b: bool| if b { Check::On } else { Check::Off };
        let has = |v: &[String], x: &str| v.iter().any(|d| d == x);
        match row {
            Row::AllDbs => on(has(&self.databases, SCOPE_ALL)),
            Row::DefaultDb => on(has(&self.databases, SCOPE_DEFAULT)),
            Row::Db(db) if has(&self.databases, db) => Check::On,
            Row::Db(db) if has(&self.databases, SCOPE_ALL) || (has(&self.databases, SCOPE_DEFAULT) && *db == self.default_db) => Check::Implicit,
            Row::Db(_) | Row::Note(..) => Check::Off,
            Row::AllSchemas(db) => on(has(&self.sch_list(db), SCOPE_ALL)),
            Row::DefaultSchema(db) => on(has(&self.sch_list(db), SCOPE_DEFAULT)),
            Row::Schema { db, name, .. } => {
                let cur = self.sch_list(db);
                on(has(&cur, SCOPE_ALL) || has(&cur, name) || (has(&cur, SCOPE_DEFAULT) && name == "public"))
            }
        }
    }

    /// Space: toggle the selected line
    pub fn toggle(&mut self) {
        fn flip(v: &mut Vec<String>, x: &str) {
            if let Some(i) = v.iter().position(|d| d == x) {
                v.remove(i);
            } else {
                v.push(x.to_string());
            }
        }
        let Some(row) = self.rows().get(self.sel).cloned() else { return };
        let (db, x) = match row {
            Row::AllDbs => return flip(&mut self.databases, SCOPE_ALL),
            Row::DefaultDb => return flip(&mut self.databases, SCOPE_DEFAULT),
            Row::Db(db) => return flip(&mut self.databases, &db),
            Row::Note(..) => return,
            Row::AllSchemas(db) => (db, SCOPE_ALL.to_string()),
            Row::DefaultSchema(db) => (db, SCOPE_DEFAULT.to_string()),
            Row::Schema { db, name, .. } => (db, name),
        };
        let mut cur = self.sch_list(&db);
        flip(&mut cur, &x);
        if cur.is_empty() {
            self.schemas.remove(&db);
        } else {
            self.schemas.insert(db, cur);
        }
    }

    /// →: expand the DB. If its schemas aren't loaded yet, the DB name (to load)
    pub fn expand(&mut self) -> Option<String> {
        let Some(Row::Db(db)) = self.rows().get(self.sel).cloned() else { return None };
        self.open.insert(db.clone());
        (!self.loaded.contains_key(&db)).then_some(db)
    }

    /// ←: collapse the DB (from a schema line, go to its DB)
    pub fn collapse(&mut self) {
        let parent = match self.rows().get(self.sel).cloned() {
            Some(Row::Db(db)) => {
                self.open.remove(&db);
                return;
            }
            Some(Row::AllSchemas(db) | Row::DefaultSchema(db) | Row::Schema { db, .. } | Row::Note(db, _)) => db,
            _ => return,
        };
        self.sel = self.rows().iter().position(|r| *r == Row::Db(parent.clone())).unwrap_or(self.sel);
    }

    pub fn set_schemas(&mut self, db: &str, r: Result<Vec<SchemaInfo>, String>) {
        self.loaded.insert(db.to_string(), r);
    }

    pub fn move_by(&mut self, d: isize) {
        let n = self.rows().len();
        self.sel = (self.sel as isize + d).clamp(0, n.saturating_sub(1) as isize) as usize;
    }

    /// Scope to save ("all" is normalized to no filter)
    pub fn scope(&self) -> Scope {
        let all = |v: &Vec<String>| v.len() == 1 && v[0] == SCOPE_ALL;
        Scope {
            databases: (!self.databases.is_empty() && !all(&self.databases)).then(|| self.databases.clone()),
            schemas: self.schemas.iter().filter(|(_, v)| !all(v)).map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sch(names: &[(&str, bool)]) -> Vec<SchemaInfo> {
        names.iter().map(|(n, s)| SchemaInfo { name: n.to_string(), is_system: *s }).collect()
    }

    fn editor(scope: Scope) -> ScopeEditor {
        ScopeEditor::new("c1".into(), vec!["app".into(), "postgres".into(), "shop".into()], "postgres".into(), &scope, None)
    }

    fn select(e: &mut ScopeEditor, row: Row) {
        e.sel = e.rows().iter().position(|r| *r == row).unwrap_or_else(|| panic!("no row {row:?} in {:?}", e.rows()));
    }

    /// With no scope, only All databases is checked, and DBs show by that rule
    #[test]
    fn starts_from_all_databases() {
        let e = editor(Scope::default());
        assert_eq!(e.rows(), vec![Row::AllDbs, Row::DefaultDb, Row::Db("app".into()), Row::Db("postgres".into()), Row::Db("shop".into())]);
        assert_eq!(e.check(&Row::AllDbs), Check::On);
        assert_eq!(e.check(&Row::DefaultDb), Check::Off);
        assert_eq!(e.check(&Row::Db("shop".into())), Check::Implicit);
        assert_eq!(e.scope(), Scope::default());
    }

    /// Pick a DB to show: turning off All databases and on shop shows only shop
    #[test]
    fn pick_databases() {
        let mut e = editor(Scope::default());
        select(&mut e, Row::AllDbs);
        e.toggle();
        select(&mut e, Row::Db("shop".into()));
        e.toggle();
        assert_eq!(e.check(&Row::Db("shop".into())), Check::On);
        assert_eq!(e.check(&Row::Db("app".into())), Check::Off);
        assert_eq!(e.scope().databases, Some(vec!["shop".to_string()]));
        // Turning on Default database also shows postgres (by the rule)
        select(&mut e, Row::DefaultDb);
        e.toggle();
        assert_eq!(e.check(&Row::Db("postgres".into())), Check::Implicit);
        assert_eq!(e.scope().databases, Some(vec!["shop".to_string(), SCOPE_DEFAULT.to_string()]));
    }

    /// Expanding a DB loads its schemas (system schemas first); picking just one limits that DB to that schema
    #[test]
    fn pick_schemas_of_a_database() {
        let mut e = editor(Scope::default());
        select(&mut e, Row::Db("shop".into()));
        assert_eq!(e.expand(), Some("shop".into()));
        assert!(e.rows().contains(&Row::Note("shop".into(), "Loading…".into())));
        e.set_schemas("shop", Ok(sch(&[("public", false), ("sales", false), ("pg_catalog", true)])));
        let rows = e.rows();
        let i = rows.iter().position(|r| *r == Row::Db("shop".into())).unwrap();
        assert_eq!(
            rows[i + 1..].to_vec(),
            vec![
                Row::AllSchemas("shop".into()),
                Row::DefaultSchema("shop".into()),
                Row::Schema { db: "shop".into(), name: "pg_catalog".into(), system: true },
                Row::Schema { db: "shop".into(), name: "public".into(), system: false },
                Row::Schema { db: "shop".into(), name: "sales".into(), system: false },
            ]
        );
        // With no filter, All schemas is checked
        assert_eq!(e.check(&Row::AllSchemas("shop".into())), Check::On);
        // Same order as the GUI: turn on sales and turn off All schemas (turning everything off goes back to "all")
        select(&mut e, Row::Schema { db: "shop".into(), name: "sales".into(), system: false });
        e.toggle();
        select(&mut e, Row::AllSchemas("shop".into()));
        e.toggle();
        assert_eq!(e.check(&Row::Schema { db: "shop".into(), name: "public".into(), system: false }), Check::Off);
        assert_eq!(e.scope().schemas.get("shop"), Some(&vec!["sales".to_string()]));
        // On a schema line, ← goes to the DB, and another ← collapses it
        e.collapse();
        assert_eq!(e.rows()[e.sel], Row::Db("shop".into()));
        e.collapse();
        assert!(!e.rows().contains(&Row::AllSchemas("shop".into())));
        // Once loaded, expanding again doesn't fetch again
        assert_eq!(e.expand(), None);
    }

    /// Starting from the current scope: Default schema shows as public checked
    #[test]
    fn starts_from_saved_scope() {
        let scope = Scope { databases: Some(vec!["shop".into()]), schemas: HashMap::from([("shop".to_string(), vec![SCOPE_DEFAULT.to_string()])]) };
        let mut e = ScopeEditor::new("c1".into(), vec!["app".into(), "shop".into()], "app".into(), &scope, Some("shop"));
        assert_eq!(e.rows()[e.sel], Row::Db("shop".into()));
        e.set_schemas("shop", Ok(sch(&[("public", false), ("sales", false)])));
        assert_eq!(e.check(&Row::AllDbs), Check::Off);
        assert_eq!(e.check(&Row::DefaultSchema("shop".into())), Check::On);
        assert_eq!(e.check(&Row::Schema { db: "shop".into(), name: "public".into(), system: false }), Check::On);
        assert_eq!(e.check(&Row::Schema { db: "shop".into(), name: "sales".into(), system: false }), Check::Off);
        assert_eq!(e.scope(), scope);
    }

    /// On failure, show the error on that line
    #[test]
    fn schema_load_error_is_shown() {
        let mut e = editor(Scope::default());
        select(&mut e, Row::Db("app".into()));
        e.expand();
        e.set_schemas("app", Err("permission denied".into()));
        assert!(e.rows().contains(&Row::Note("app".into(), "permission denied".into())));
    }
}
