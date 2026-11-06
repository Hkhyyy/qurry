//! Import Data Sources from DataGrip / IntelliJ and DBeaver: the fixtures under tests/fixtures/import are a fake home folder
//! (hand-written files in the tools' formats) and a few hostile files
use qurry_core::import::{locations, ImportPlan, ImportRow};
use qurry_core::store::{ConnectionConfig, DbKind, SshAuth, Store};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import")
}

/// The import checks "already in Qurry" again under the store's lock, with the tunnel as part of the source: the same server
/// reached directly isn't the one through a bastion, the same one through the same bastion is
#[test]
fn import_rechecks_duplicates_with_the_tunnel() {
    let ws = home().join(".var/app/io.dbeaver.DBeaverCommunity/data/DBeaverData/workspace6/Ops/.dbeaver");
    let direct: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "d", "name": "ops direct", "kind": "postgres", "host": "ops-db.internal", "port": 5432, "user": "", "database": "ops" })).unwrap();
    let mut tunneled = direct.clone();
    tunneled.id = "t".into();
    tunneled.ssh = Some(serde_json::from_value(serde_json::json!({ "enabled": true, "host": "ops-gw.example.org", "port": 2222, "user": "" })).unwrap());
    let (s, _d) = store();
    let plan = s.import_plan(&ws, &home(), None).unwrap();
    assert_eq!(plan.importable, 1);
    // Added by another app between the preview and the import: reached directly, so ops-pg is still new
    s.save(&direct).unwrap();
    assert_eq!(s.import_apply(&plan, &[], |_| Ok(())).unwrap().0, 1);
    let (s, _d) = store();
    let plan = s.import_plan(&ws, &home(), None).unwrap();
    // Through the same tunnel: it's the same source and isn't added twice
    s.save(&tunneled).unwrap();
    assert_eq!(s.import_apply(&plan, &[], |_| Ok(())).unwrap().0, 0);
}

fn home() -> PathBuf {
    fixtures().join("home")
}

fn datagrip_global() -> PathBuf {
    home().join("Library/Application Support/JetBrains/DataGrip2025.2/options")
}

/// An empty Qurry config folder
fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (Store::new(dir.path().to_path_buf()), dir)
}

fn row<'a>(plan: &'a ImportPlan, name: &str) -> &'a ImportRow {
    plan.rows.iter().find(|r| r.text.contains(&format!("\"{name}\""))).unwrap_or_else(|| panic!("no row {name}: {:#?}", plan.rows))
}

fn saved(s: &Store, name: &str) -> ConnectionConfig {
    s.list().unwrap().into_iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name} wasn't imported"))
}

#[test]
fn datagrip_global_with_local_and_ssh_configs() {
    let (s, _d) = store();
    let mut billing = saved_billing();
    billing.id = "mine".into();
    s.save(&billing).unwrap();
    let plan = s.import_plan(&datagrip_global(), &home(), None).unwrap();
    assert!(plan.message.starts_with("Import these data sources from DataGrip 2025.2 · global?\n~/Library/Application Support/JetBrains/DataGrip2025.2/options\n\n8 to add, 2 skipped."), "{}", plan.message);
    assert!(plan.message.ends_with(qurry_core::import::NOTE));
    assert_eq!(plan.importable, 8);

    let r = row(&plan, "orders-prod");
    assert_eq!(r.text, "+ add \"orders-prod\" (app@orders-db.internal:5432/orders)");
    assert_eq!(r.note, "PostgreSQL · SSH ec2-user@bastion.example.com (key ~/.ssh/id_ed25519) · SSL verify-full (CA ~/certs/orders-ca.pem) · Read-only");
    assert_eq!((r.value.as_str(), r.tone.as_deref(), r.checkable), ("password on connect", None, true));
    let r = row(&plan, "analytics");
    assert_eq!((r.value.as_str(), r.tone.as_deref()), ("SSL without certificate checks", Some("warn")));
    let r = row(&plan, "events");
    assert!(r.note.contains("SSH deploy@jump-eu.example.com:2222 (key)") && r.value.contains("OpenSSH config / agent"), "{r:?}");
    assert_eq!(row(&plan, "local-pg").value, "no password");
    assert_eq!(row(&plan, "cache").text, "+ add \"cache\" (localhost:6379/0)");
    let r = row(&plan, "warehouse");
    assert_eq!((r.text.as_str(), r.note.as_str(), r.value.as_str(), r.checkable), ("− skip \"warehouse\" (Snowflake)", "Snowflake isn't supported", "skipped", false));
    let r = row(&plan, "billing");
    assert_eq!((r.note.as_str(), r.checkable), ("already in Qurry (same server, user and database)", false));

    // Leave out "events" and "cache": only the checked rows are imported, with fresh ids, MCP Off and no password
    let picked: Vec<bool> = plan.rows.iter().map(|r| !r.text.contains("\"events\"") && !r.text.contains("\"cache\"")).collect();
    let (added, updated, forget) = s.import_apply(&plan, &picked, |_| Ok(())).unwrap();
    assert_eq!((added, updated, forget.len()), (6, 0, 0));
    let names: Vec<String> = s.list().unwrap().into_iter().map(|c| c.name).collect();
    assert!(!names.contains(&"events".to_string()) && !names.contains(&"cache".to_string()) && !names.contains(&"warehouse".to_string()), "{names:?}");
    let o = saved(&s, "orders-prod");
    assert_eq!((o.kind, o.host.as_str(), o.port, o.user.as_str(), o.database.as_str(), o.read_only), (DbKind::Postgres, "orders-db.internal", 5432, "app", "orders", true));
    assert!(!o.id.is_empty() && o.id != "0b6f5c1e-1111-4a5e-9d2a-000000000001" && !o.password_saved && !o.ssh_secret_saved);
    assert_eq!(o.options.mcp.as_deref(), Some("off"));
    let ssh = o.ssh.unwrap();
    assert_eq!((ssh.enabled, ssh.host.as_str(), ssh.port, ssh.user.as_str(), ssh.auth, ssh.key_path.as_str()), (true, "bastion.example.com", 22, "ec2-user", SshAuth::Key, "~/.ssh/id_ed25519"));
    let ssl = o.ssl.unwrap();
    assert_eq!((ssl.enabled, ssl.mode.as_str(), ssl.ca_file.as_str()), (true, "verify-full", "~/certs/orders-ca.pem"));
    let m = saved(&s, "legacy");
    assert_eq!((m.kind, m.host.as_str(), m.port, m.database.as_str(), m.user.as_str()), (DbKind::Mssql, "mssql.internal", 1433, "legacy", "sa"));
    let hr = saved(&s, "hr");
    assert_eq!((hr.kind, hr.host.as_str(), hr.database.as_str()), (DbKind::Oracle, "ora.internal", "ORCLPDB1"));
    let f = saved(&s, "app.db");
    assert_eq!((f.kind, f.host.as_str(), f.database.as_str()), (DbKind::Sqlite, "", "/var/data/app.db"));

    // Importing again: what was added is "already in Qurry"
    let again = s.import_plan(&datagrip_global(), &home(), None).unwrap();
    assert_eq!(again.importable, 2, "{:#?}", again.rows);
}

fn saved_billing() -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({ "id": "x", "name": "Billing (mine)", "kind": "postgres", "host": "Billing-DB.internal", "port": 5432, "user": "billing", "database": "billing" })).unwrap()
}

#[test]
fn datagrip_project_with_project_ssh_config_and_inline_tunnel() {
    let (s, _d) = store();
    let idea = home().join("work/shop-api/.idea");
    // The project folder, its .idea and its dataSources.xml all read the same
    for p in [idea.parent().unwrap().to_path_buf(), idea.clone(), idea.join("dataSources.xml"), idea.join("dataSources.local.xml")] {
        let plan = s.import_plan(&p, &home(), None).unwrap();
        assert!(plan.message.starts_with("Import these data sources from DataGrip / IntelliJ · project shop-api?\n~/work/shop-api"), "{}", plan.message);
        assert_eq!(plan.importable, 3);
    }
    let plan = s.import_plan(&idea, &home(), None).unwrap();
    assert_eq!(row(&plan, "shop-pg").note, "PostgreSQL · SSH shop-ops@shop-bastion.example.com (password)");
    assert_eq!(row(&plan, "shop-mongo").text, "+ add \"shop-mongo\" (mongo.internal:27017/shop)");
    let key = format!("{}/keys/legacy.pem", idea.parent().unwrap().display());
    assert_eq!(row(&plan, "shop-legacy").note, format!("MySQL · SSH ops@gw.example.net:2200 (key {key})"));
    s.import_apply(&plan, &[], |_| Ok(())).unwrap();
    let l = saved(&s, "shop-legacy");
    assert_eq!((l.port, l.ssh.as_ref().unwrap().auth), (3306, SshAuth::Key));
    assert_eq!(saved(&s, "shop-mongo").kind, DbKind::Mongodb);
}

#[test]
fn dbeaver_with_ssh_ssl_oracle_mongodb_and_unsupported() {
    let (s, _d) = store();
    let dir = home().join("Library/DBeaverData/workspace6/General/.dbeaver");
    let plan = s.import_plan(&dir, &home(), None).unwrap();
    assert!(plan.message.starts_with("Import these data sources from DBeaver · General?\n~/Library/DBeaverData/workspace6/General/.dbeaver\n\n5 to add, 2 skipped."), "{}", plan.message);
    let r = row(&plan, "prod-replica");
    assert_eq!(r.text, "+ add \"prod-replica\" (10.20.0.15:5432/app)");
    let project = home().join("Library/DBeaverData/workspace6/General");
    assert_eq!(r.note, format!("PostgreSQL · Production · SSH deploy@jump.example.org (password) · SSL verify-full (CA {}/certs/root.crt) · Read-only", project.display()));
    assert_eq!((r.value.as_str(), r.tone.as_deref()), ("no user name in the file (DBeaver keeps it with the password)", Some("warn")));
    assert_eq!(row(&plan, "Sakila").value, "password on connect");
    let r = row(&plan, "HR");
    assert_eq!((r.text.as_str(), r.value.as_str()), ("+ add \"HR\" (ora.internal:1521/ORCL)", "Oracle SID used as the service name; no user name in the file (DBeaver keeps it with the password)"));
    assert!(row(&plan, "logs").note.starts_with("MongoDB · SSL require"));
    let r = row(&plan, "DB2 Sample");
    assert_eq!((r.text.as_str(), r.note.as_str(), r.checkable), ("− skip \"DB2 Sample\" (Db2)", "Db2 isn't supported", false));
    let r = row(&plan, "CSV exports");
    assert_eq!((r.text.as_str(), r.note.as_str()), ("− skip \"CSV exports\" (Flat files (CSV))", "file drivers aren't supported"));

    s.import_apply(&plan, &[], |_| Ok(())).unwrap();
    let p = saved(&s, "prod-replica");
    assert!(p.read_only && p.options.confirm_changes && p.options.mcp.as_deref() == Some("off"));
    let ssh = p.ssh.unwrap();
    assert_eq!((ssh.host.as_str(), ssh.user.as_str(), ssh.auth), ("jump.example.org", "deploy", SshAuth::Password));
    assert_eq!(saved(&s, "Sakila").user, "root");
    assert_eq!(saved(&s, "chinook").database, "/var/data/chinook.db");

    // A DBeaver workspace (Flatpak): the projects' .dbeaver folders, data-sources-2.json too
    let ws = home().join(".var/app/io.dbeaver.DBeaverCommunity/data/DBeaverData/workspace6");
    let plan = s.import_plan(&ws, &home(), None).unwrap();
    let r = row(&plan, "ops-pg");
    let ops = ws.join("Ops");
    assert_eq!(r.note, format!("PostgreSQL · SSH ops-gw.example.org:2222 (key {}/keys/ops_ed25519) · SSL verify-ca (CA /etc/ssl/ops-ca.pem)", ops.display()));
    s.import_apply(&plan, &[], |_| Ok(())).unwrap();
    let ssl = saved(&s, "ops-pg").ssl.unwrap();
    assert_eq!((ssl.cert_file.as_str(), ssl.key_file.as_str()), ("/etc/ssl/ops-client.crt", "/etc/ssl/ops-client.key"));
}

#[test]
fn passwords_in_the_files_are_not_imported() {
    // In the files: password elements and keys, and passwords in JDBC URLs (query, user info with / , @, MySQL's (…) host form)
    let secrets = ["url-secret-1", "xml-secret-2", "local-secret-3", "ssh-secret-4", "json-secret-5", "json-ssh-secret-6", "json-ssh-secret-7", "secret,8", "url-secret-9", "url-secret-10"];
    for (p, n) in [(fixtures().join("password"), 3), (fixtures().join("password/data-sources.json"), 1)] {
        let (s, d) = store();
        let plan = s.import_plan(&p, &home(), None).unwrap();
        assert_eq!(plan.importable, n, "{p:?}: {:#?}", plan.rows);
        s.import_apply(&plan, &[], |_| Ok(())).unwrap();
        let c = saved(&s, "with-password");
        assert!(!c.password_saved && !c.ssh_secret_saved);
        // Nowhere: not in the preview, not in anything written to the config folder
        let shown = serde_json::to_string(&plan).unwrap();
        for e in std::fs::read_dir(d.path()).unwrap().flatten() {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            for secret in secrets {
                assert!(!text.contains(secret) && !shown.contains(secret), "{secret} in {:?}", e.path());
            }
        }
    }
}

#[test]
fn malformed_files_say_where() {
    let (s, _d) = store();
    let e = s.import_plan(&fixtures().join("broken/dataSources.xml"), &home(), None).unwrap_err().to_string();
    assert!(e.starts_with("Can't read ") && e.contains("broken/dataSources.xml:\nline 7, column ") && e.ends_with("Nothing was imported."), "{e}");
    let e = s.import_plan(&fixtures().join("broken/data-sources.json"), &home(), None).unwrap_err().to_string();
    assert!(e.contains("line 7, column 4: expected `,` or `}`"), "{e}");
    let e = s.import_plan(&home().join("work/gone-project"), &home(), None).unwrap_err().to_string();
    assert_eq!(e, "Can't import: ~/work/gone-project isn't there anymore (moved or deleted).");
    let e = s.import_plan(&home().join("work"), &home(), None).unwrap_err().to_string();
    assert_eq!(e, "Can't import: ~/work has no Qurry export, dataSources.xml, .idea folder or data-sources.json.");
}

#[test]
fn external_entities_are_refused() {
    let (s, _d) = store();
    let e = s.import_plan(&fixtures().join("xxe"), &home(), None).unwrap_err().to_string();
    assert!(e.contains("DTDs and entities aren't read") && !e.contains("root:"), "{e}");
}

#[test]
fn locations_newest_first_with_counts() {
    let found = locations(&home());
    let got: Vec<(String, String, usize)> = found.iter().map(|l| (l.label.clone(), l.shown.clone(), l.count)).collect();
    let jb = "~/Library/Application Support/JetBrains";
    let want = [
        ("DataGrip 2025.2 · global".to_string(), format!("{jb}/DataGrip2025.2/options"), 10),
        ("DataGrip · project default".to_string(), format!("{jb}/DataGrip2025.2/projects/default/.idea"), 1),
        ("IntelliJ IDEA 2025.2 · global".to_string(), format!("{jb}/IntelliJIdea2025.2/options"), 2),
        // From IntelliJ's recentProjects.xml; the project that is gone isn't listed
        ("IntelliJ IDEA · project shop-api".to_string(), "~/work/shop-api/.idea".to_string(), 3),
        ("DataGrip 2025.1 · global".to_string(), "~/.config/JetBrains/DataGrip2025.1/options".to_string(), 1),
        ("DataGrip 2024.3 · global".to_string(), format!("{jb}/DataGrip2024.3/options"), 1),
        ("DataGrip · project analytics".to_string(), "~/DataGripProjects/analytics/.idea".to_string(), 1),
        ("DBeaver · workspace6 General".to_string(), "~/Library/DBeaverData/workspace6/General/.dbeaver".to_string(), 7),
        ("DBeaver (Flatpak) · workspace6 Ops".to_string(), "~/.var/app/io.dbeaver.DBeaverCommunity/data/DBeaverData/workspace6/Ops/.dbeaver".to_string(), 1),
    ];
    assert_eq!(got, want.to_vec());
    assert!(locations(Path::new("/nonexistent-home")).is_empty());
}

#[test]
fn qurry_export_is_read_like_before() {
    let (a, d) = store();
    a.save(&saved_billing()).unwrap();
    let file = d.path().join(qurry_core::import::EXPORT_NAME);
    a.export_to(&file).unwrap();
    let (b, _e) = store();
    // The folder with the export in it reads the export
    let plan = b.import_plan(d.path(), &home(), None).unwrap();
    assert!(plan.message.starts_with("Import these data sources from a Qurry export?"), "{}", plan.message);
    assert_eq!(plan.rows[0].text, "+ add \"Billing (mine)\" (billing@Billing-DB.internal:5432/billing)");
    // Unchecked: nothing imported
    assert_eq!(b.import_apply(&plan, &[false], |_| Ok(())).unwrap(), (0, 0, vec![]));
    assert_eq!(b.import_apply(&plan, &[true], |_| Ok(())).unwrap(), (1, 0, vec![]));
}
