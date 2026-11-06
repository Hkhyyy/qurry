//! Import Data Sources: Qurry's own export, DataGrip / IntelliJ (dataSources.xml + dataSources.local.xml + sshConfigs.xml) and
//! DBeaver (data-sources*.json), and the places on this computer those tools keep them (`locations`).
//! The files are read as untrusted input: a size limit, no DTD or entities beyond XML's five, only the file names each format
//! uses inside the folder picked. Passwords are never read, even when a file has one: they are asked on the first connect
use crate::error::{AppError, AppResult};
use crate::store::{ConnectionConfig, DbKind, ExportFile, SshAuth, SshConfig, SslConfig};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Bigger files aren't read (a dataSources.local.xml with a large schema mapping is well under a megabyte)
const MAX_FILE: u64 = 8 << 20;
/// Data sources read from one location at most
const MAX_SOURCES: usize = 5000;
/// Entries read from one folder or recentProjects.xml at most (a home folder with thousands of projects isn't walked through)
const MAX_ENTRIES: usize = 300;
/// The file name Qurry's Export Data Sources… suggests (found when a folder is picked)
pub const EXPORT_NAME: &str = "qurry-datasources.json";
/// Said once above the list (GUI and TUI)
pub const NOTE: &str = "New data sources start with MCP Off. Passwords aren't imported: each asks on its first connect.";

/// A location found on this computer: the submenu / palette lists these, newest IDE version first
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// "DataGrip 2025.2 · global", "IntelliJ IDEA · project shop-api", "DBeaver · workspace6 General"
    pub label: String,
    /// The folder to read (an IDE options folder, a project's .idea, DBeaver's .dbeaver)
    pub path: String,
    /// The path as shown (~ for the home folder)
    pub shown: String,
    /// Data sources in it (including the ones that would be skipped)
    pub count: usize,
}

/// One line of the import confirm (GUI detail list, TUI rows)
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    /// `+ add "orders-prod" (app@orders-db.internal:5432/orders)`, `~ update "x"`, `− skip "warehouse" (Snowflake)`
    pub text: String,
    /// What was mapped (type · SSH · SSL · Read-only), the changes of an update, or why it's skipped
    pub note: String,
    /// "password on connect", "no password", a warning, "skipped"
    pub value: String,
    /// "warn" for a warning
    pub tone: Option<String>,
    /// Added and updated rows can be left out; skipped ones can't be picked
    pub checkable: bool,
}

/// What an import would do, read once: the confirm shows `rows`, and importing applies the checked rows' configs
/// (re-checked against the store then, under its lock)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub title: String,
    /// The question (or why there's nothing to import), the path, the counts and NOTE
    pub message: String,
    pub rows: Vec<ImportRow>,
    /// Rows that can be imported; 0 = a notice (OK only) instead of the confirm
    pub importable: usize,
    #[serde(skip)]
    pub(crate) configs: Vec<Option<ConnectionConfig>>,
}

pub const TITLE: &str = "Import Data Sources";

impl ImportPlan {
    /// Status text after importing
    pub fn done_message(added: usize, updated: usize) -> String {
        let mut s = format!("Imported {added} data source{}", if added == 1 { "" } else { "s" });
        if updated > 0 {
            s += &format!(", updated {updated}");
        }
        s + " — passwords are asked on the first connect"
    }
}

/// A data source read from DataGrip or DBeaver: what Qurry will save, or why it's skipped
pub(crate) struct Foreign {
    pub name: String,
    pub kind_label: String,
    pub what: Result<Mapped, String>,
}

pub(crate) struct Mapped {
    pub config: ConnectionConfig,
    /// "SSH ec2-user@bastion.example.com (key ~/.ssh/id_ed25519)", "SSL verify-full", "Read-only"
    pub mapped: Vec<String>,
    pub warn: Vec<String>,
    /// The tool connects without a password (DataGrip's No auth): said instead of "password on connect"
    pub no_password: bool,
}

pub(crate) enum Read {
    Qurry(Vec<ConnectionConfig>),
    Foreign(Vec<Foreign>),
}

/// The user's home folder (where the other tools keep their settings)
pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// Show a path with ~ for the home folder
pub fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

fn gone(path: &Path, home: &Path) -> AppError {
    AppError::Msg(format!("Can't import: {} isn't there anymore (moved or deleted).", shown(path, home)))
}

/// Read a file of at most MAX_FILE
fn read_text(path: &Path, home: &Path) -> AppResult<String> {
    use std::io::Read as _;
    let f = std::fs::File::open(path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { gone(path, home) } else { e.into() })?;
    let mut buf = Vec::new();
    f.take(MAX_FILE + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_FILE {
        return Err(AppError::Msg(format!("Can't import {}: the file is larger than {} MB.", shown(path, home), MAX_FILE >> 20)));
    }
    String::from_utf8(buf).map_err(|_| AppError::Msg(format!("Can't read {}: it isn't UTF-8 text.", shown(path, home))))
}

fn bad_file(path: &Path, home: &Path, at: String) -> AppError {
    AppError::Msg(format!("Can't read {}:\n{at}\n\nNothing was imported.", shown(path, home)))
}

fn read_json(path: &Path, home: &Path) -> AppResult<Value> {
    serde_json::from_str(&read_text(path, home)?).map_err(|e| {
        let s = e.to_string();
        let what = s.rsplit_once(" at line ").map(|x| x.0.to_string()).unwrap_or(s);
        bad_file(path, home, format!("line {}, column {}: {what}", e.line(), e.column()))
    })
}

// ───────── A small XML reader (no DTD, no entities but XML's own) ─────────

#[derive(Debug, Default)]
pub(crate) struct El {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<El>,
    text: String,
}

impl El {
    fn attr(&self, n: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == n).map(|a| a.1.as_str())
    }
    fn child(&self, n: &str) -> Option<&El> {
        self.kids.iter().find(|k| k.name == n)
    }
    /// A child's trimmed text, None when missing or empty
    fn text_of(&self, n: &str) -> Option<&str> {
        self.child(n).map(|c| c.text.trim()).filter(|t| !t.is_empty())
    }
    /// Every element with this name below (depth first)
    fn all<'a>(&'a self, n: &str, out: &mut Vec<&'a El>) {
        for k in &self.kids {
            if k.name == n {
                out.push(k);
            } else {
                k.all(n, out);
            }
        }
    }
}

/// "line L, column C" of a byte offset
fn line_col(text: &str, pos: usize) -> String {
    let before = &text[..text.floor_char_boundary(pos.min(text.len()))];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
    format!("line {line}, column {col}")
}

pub(crate) fn parse_xml(text: &str) -> Result<El, String> {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_str(text);
    let mut stack = vec![El::default()];
    let at = |pos: u64, msg: String| format!("{}: {msg}", line_col(text, pos as usize));
    let element = |e: &quick_xml::events::BytesStart, r: &quick_xml::Reader<&[u8]>| -> Result<El, String> {
        let mut el = El { name: e.name().as_ref().to_string(), ..Default::default() };
        for a in e.attributes() {
            let a = a.map_err(|x| at(r.buffer_position(), x.to_string()))?;
            let v = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|x| at(r.buffer_position(), x.to_string()))?;
            el.attrs.push((a.key.as_ref().to_string(), v.into_owned()));
        }
        Ok(el)
    };
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
                if stack.len() > 64 {
                    return Err(at(r.buffer_position(), "the elements are nested too deep".into()));
                }
                let el = element(&e, &r)?;
                stack.push(el);
            }
            Ok(Event::Empty(e)) => {
                let el = element(&e, &r)?;
                stack.last_mut().unwrap().kids.push(el);
            }
            Ok(Event::End(_)) => {
                let el = stack.pop().unwrap();
                match stack.last_mut() {
                    Some(p) => p.kids.push(el),
                    None => return Err(at(r.buffer_position(), "an end tag without a start".into())),
                }
            }
            Ok(Event::Text(t)) => stack.last_mut().unwrap().text.push_str(&t.xml10_content()),
            Ok(Event::CData(t)) => stack.last_mut().unwrap().text.push_str(&t),
            Ok(Event::GeneralRef(e)) => {
                let s = if e.is_char_ref() {
                    e.resolve_char_ref().ok().flatten().map(String::from)
                } else {
                    quick_xml::escape::resolve_predefined_entity(&e).map(String::from)
                };
                match s {
                    Some(s) => stack.last_mut().unwrap().text.push_str(&s),
                    None => return Err(at(r.buffer_position(), format!("the entity &{}; isn't defined (entities aren't read)", &*e))),
                }
            }
            // XXE and entity expansion: an IDE never writes a DTD, so a file that has one isn't read at all
            Ok(Event::DocType(_)) => return Err(at(r.buffer_position(), "the file has a DTD (<!DOCTYPE …>): DTDs and entities aren't read".into())),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(at(r.error_position(), e.to_string())),
        }
    }
    if stack.len() > 1 {
        return Err(at(text.len() as u64, format!("the element <{}> isn't closed", stack.last().unwrap().name)));
    }
    Ok(stack.pop().unwrap())
}

fn read_xml(path: &Path, home: &Path) -> AppResult<El> {
    let text = read_text(path, home)?;
    parse_xml(&text).map_err(|at| bad_file(path, home, at))
}

// ───────── Format detection ─────────

fn is_dbeaver_file(name: &str) -> bool {
    name == "data-sources.json" || (name.starts_with("data-sources-") && name.ends_with(".json"))
}

/// DBeaver's data-sources*.json files in a .dbeaver folder (sorted, so the order is stable)
fn dbeaver_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .take(MAX_ENTRIES)
        .filter(|e| e.file_name().to_str().is_some_and(is_dbeaver_file))
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    v
}

/// Sub-folders, sorted, at most MAX_ENTRIES
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().take(MAX_ENTRIES).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    v.sort();
    v
}

/// Read whatever `path` is: a Qurry export, dataSources.xml / dataSources.local.xml, a project (or its .idea), an IDE options
/// folder, DBeaver's data-sources*.json, a .dbeaver folder, a DBeaver project or workspace folder
pub(crate) fn read(path: &Path, home: &Path) -> AppResult<Read> {
    if !path.exists() {
        return Err(gone(path, home));
    }
    let nothing = || AppError::Msg(format!("Can't import: {} has no Qurry export, dataSources.xml, .idea folder or data-sources.json.", shown(path, home)));
    if path.is_dir() {
        if path.join("dataSources.xml").is_file() {
            return Ok(Read::Foreign(read_jetbrains(path, home)?));
        }
        if path.join(".idea/dataSources.xml").is_file() {
            return Ok(Read::Foreign(read_jetbrains(&path.join(".idea"), home)?));
        }
        let files = dbeaver_files(path);
        if !files.is_empty() {
            return Ok(Read::Foreign(read_dbeaver(&files, home)?));
        }
        // A DBeaver project, or a workspace with projects
        let mut files = dbeaver_files(&path.join(".dbeaver"));
        if files.is_empty() {
            files = subdirs(path).iter().flat_map(|p| dbeaver_files(&p.join(".dbeaver"))).collect();
        }
        if !files.is_empty() {
            return Ok(Read::Foreign(read_dbeaver(&files, home)?));
        }
        if path.join(EXPORT_NAME).is_file() {
            return read(&path.join(EXPORT_NAME), home);
        }
        return Err(nothing());
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if name == "dataSources.xml" || name == "dataSources.local.xml" {
        return Ok(Read::Foreign(read_jetbrains(path.parent().unwrap_or(Path::new(".")), home)?));
    }
    if !name.to_ascii_lowercase().ends_with(".json") {
        return Err(AppError::Msg(format!("Can't import {}: pick a Qurry export (.json), dataSources.xml or DBeaver's data-sources.json.", shown(path, home))));
    }
    let v = read_json(path, home)?;
    match v.get("connections") {
        Some(Value::Object(_)) => Ok(Read::Foreign(read_dbeaver(&[path.to_path_buf()], home)?)),
        _ => {
            // Qurry: { app, version, connections: [...] } or a bare list
            let list = match serde_json::from_value::<ExportFile>(v.clone()) {
                Ok(f) => f.connections,
                Err(_) => serde_json::from_value(v).map_err(|_| AppError::Msg(format!("Can't import {}: it isn't a Qurry export or DBeaver's data-sources.json.", shown(path, home))))?,
            };
            Ok(Read::Qurry(list))
        }
    }
}

/// What a picked path is called in the confirm ("DataGrip / IntelliJ · project shop-api", "DBeaver · General", "a Qurry export")
pub fn describe(path: &Path, home: &Path) -> String {
    let base = |p: &Path| p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().map(Path::to_path_buf).unwrap_or_default() };
    let name = base(path);
    if name.ends_with(".json") && !is_dbeaver_file(&name) {
        return "a Qurry export".into();
    }
    if base(&dir) == ".dbeaver" || is_dbeaver_file(&name) {
        return format!("DBeaver · {}", dir.parent().map(base).unwrap_or_default());
    }
    let idea = if base(&dir) == ".idea" { Some(dir.clone()) } else if dir.join(".idea").is_dir() { Some(dir.join(".idea")) } else { None };
    if let Some(idea) = idea {
        return format!("DataGrip / IntelliJ · project {}", idea.parent().map(base).unwrap_or_default());
    }
    if base(&dir) == "options"
        && let Some((product, version)) = dir.parent().and_then(|p| ide_dir(&base(p)))
    {
        return format!("{} {version} · global", product_name(&product));
    }
    shown(path, home)
}

// ───────── DataGrip / IntelliJ ─────────

/// JetBrains config folder names: DataGrip2025.2 → ("DataGrip", "2025.2")
fn ide_dir(name: &str) -> Option<(String, String)> {
    let i = name.find(|c: char| c.is_ascii_digit())?;
    let (p, v) = name.split_at(i);
    (!p.is_empty() && p.chars().all(|c| c.is_ascii_alphabetic()) && v.chars().all(|c| c.is_ascii_digit() || c == '.')).then(|| (p.to_string(), v.to_string()))
}

fn product_name(p: &str) -> String {
    match p {
        "IntelliJIdea" => "IntelliJ IDEA".into(),
        "IdeaIC" => "IntelliJ IDEA CE".into(),
        p => p.into(),
    }
}

/// Where JetBrains IDEs keep their settings (macOS, Linux)
fn jetbrains_roots(home: &Path) -> [PathBuf; 2] {
    [home.join("Library/Application Support/JetBrains"), home.join(".config/JetBrains")]
}

/// The IDE config folders, newest version first: (folder, product, version)
fn ide_configs(home: &Path) -> Vec<(PathBuf, String, String)> {
    let mut v: Vec<(PathBuf, String, String)> = jetbrains_roots(home)
        .iter()
        .flat_map(|r| subdirs(r))
        .filter_map(|d| {
            let (p, ver) = ide_dir(d.file_name()?.to_str()?)?;
            Some((d, p, ver))
        })
        .collect();
    let key = |ver: &str| ver.split('.').map(|n| n.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    v.sort_by(|a, b| key(&b.2).cmp(&key(&a.2)).then(a.1.cmp(&b.1)));
    v
}

/// SSH configurations by id: the folder's own sshConfigs.xml (a project's, "visible only for this project") first, then every
/// IDE's. A file that can't be read is passed over: only a source that refers to a missing one says so
fn ssh_configs(dir: &Path, home: &Path) -> Vec<El> {
    let mut files = vec![dir.join("sshConfigs.xml")];
    files.extend(ide_configs(home).into_iter().map(|(d, ..)| d.join("options/sshConfigs.xml")));
    let mut out = vec![];
    for f in files.iter().filter(|f| f.is_file()) {
        if let Ok(root) = read_xml(f, home) {
            let mut v = vec![];
            root.all("sshConfig", &mut v);
            out.extend(v.into_iter().map(|e| El { name: e.name.clone(), attrs: e.attrs.clone(), ..Default::default() }));
        }
    }
    out
}

/// $USER_HOME$ → ~ (kept portable), $PROJECT_DIR$ → the project folder
fn expand_macros(s: &str, project: Option<&Path>) -> String {
    let s = s.replace("$USER_HOME$", "~");
    match project {
        Some(p) => s.replace("$PROJECT_DIR$", &p.display().to_string()).replace("$ProjectFileDir$", &p.display().to_string()),
        None => s,
    }
}

fn read_jetbrains(dir: &Path, home: &Path) -> AppResult<Vec<Foreign>> {
    let shared = read_xml(&dir.join("dataSources.xml"), home)?;
    let local_path = dir.join("dataSources.local.xml");
    let local = if local_path.is_file() { Some(read_xml(&local_path, home)?) } else { None };
    let project = (dir.file_name().and_then(|n| n.to_str()) == Some(".idea")).then(|| dir.parent()).flatten();
    let mut locals = vec![];
    if let Some(l) = &local {
        l.all("data-source", &mut locals);
    }
    let ssh = ssh_configs(dir, home);
    let mut sources = vec![];
    shared.all("data-source", &mut sources);
    Ok(sources
        .into_iter()
        .take(MAX_SOURCES)
        .map(|ds| {
            let uuid = ds.attr("uuid").unwrap_or_default();
            let lo = locals.iter().find(|l| !uuid.is_empty() && l.attr("uuid") == Some(uuid)).copied();
            map_jetbrains(ds, lo, &ssh, project)
        })
        .collect())
}

fn map_jetbrains(ds: &El, local: Option<&El>, ssh_cfgs: &[El], project: Option<&Path>) -> Foreign {
    let name = ds.attr("name").unwrap_or("data source").to_string();
    let url = ds.text_of("jdbc-url").unwrap_or_default();
    let driver = ds.text_of("driver-ref").unwrap_or_default();
    let Some(target) = parse_url(url) else {
        let label = unsupported_label(driver, url);
        return Foreign { name, what: Err(if url.is_empty() { "no JDBC URL in the file".into() } else { format!("{label} isn't supported") }), kind_label: label };
    };
    let pick = |n: &str| local.and_then(|l| l.text_of(n)).or_else(|| ds.text_of(n));
    let mut m = new_mapped(&name, target);
    m.config.user = pick("user-name").unwrap_or_default().to_string();
    m.no_password = local.and_then(|l| l.text_of("auth-provider")).or_else(|| ds.text_of("auth-provider")) == Some("no-auth");
    if ds.attr("read-only") == Some("true") {
        m.config.read_only = true;
    }
    let kind_label = kind_name(m.config.kind, driver);
    if let Some(sp) = local.and_then(|l| l.child("ssh-properties")).or_else(|| ds.child("ssh-properties"))
        && sp.text_of("enabled") == Some("true")
        && !m.config.kind.is_file()
    {
        let ssh = match sp.text_of("ssh-config-id") {
            Some(id) => ssh_cfgs.iter().find(|c| c.attr("id") == Some(id)).map(|c| {
                let key = c.attr("keyPath").map(|k| expand_macros(k, project)).unwrap_or_default();
                let auth = match c.attr("authType") {
                    Some("PASSWORD") => SshAuth::Password,
                    Some("KEY_PAIR") | Some("OPEN_SSH") => SshAuth::Key,
                    _ if !key.is_empty() => SshAuth::Key,
                    _ => SshAuth::Password,
                };
                let agent = c.attr("authType") == Some("OPEN_SSH") && key.is_empty();
                (c.attr("host").unwrap_or_default().to_string(), c.attr("port").and_then(|p| p.parse().ok()), c.attr("username").unwrap_or_default().to_string(), auth, key, agent)
            }),
            // Before SSH configurations (2020): the tunnel is in the data source itself
            None => sp.text_of("proxy-host").map(|h| {
                let key = sp.text_of("private-key-path").map(|k| expand_macros(k, project)).unwrap_or_default();
                let auth = if sp.text_of("use-password") == Some("false") || (!key.is_empty() && sp.text_of("use-password").is_none()) { SshAuth::Key } else { SshAuth::Password };
                (h.to_string(), sp.text_of("proxy-port").and_then(|p| p.parse().ok()), sp.text_of("user").unwrap_or_default().to_string(), auth, key, false)
            }),
        };
        match ssh {
            Some((host, port, user, auth, key_path, agent)) => {
                set_ssh(&mut m, SshConfig { enabled: true, host, port: port.unwrap_or(22), user, auth, key_path });
                if agent {
                    m.warn.push("SSH key from the OpenSSH config / agent: choose a key file".into());
                }
            }
            None => m.warn.push("its SSH configuration isn't in the file: set the tunnel before connecting".into()),
        }
    }
    if let Some(sc) = local.and_then(|l| l.child("ssl-config")).or_else(|| ds.child("ssl-config"))
        && sc.text_of("enabled") == Some("true")
    {
        let mode = match sc.text_of("mode") {
            Some("VERIFY_FULL") => "verify-full",
            Some("VERIFY_CA") => "verify-ca",
            _ => "require",
        };
        let file = |n: &str| sc.text_of(n).map(|p| expand_macros(p, project)).unwrap_or_default();
        set_ssl(&mut m, mode, file("ca-cert"), file("client-cert"), file("client-key"));
    }
    Foreign { name, kind_label, what: Ok(finish(m)) }
}

// ───────── DBeaver ─────────

fn read_dbeaver(files: &[PathBuf], home: &Path) -> AppResult<Vec<Foreign>> {
    let mut out = vec![];
    for f in files {
        let v = read_json(f, home)?;
        let Some(conns) = v.get("connections").and_then(Value::as_object) else {
            return Err(AppError::Msg(format!("Can't import {}: it has no \"connections\" (not DBeaver's data-sources.json?).", shown(f, home))));
        };
        // ${project.path}: the DBeaver project folder (the parent of .dbeaver)
        let project = f.parent().and_then(Path::parent).map(|p| p.display().to_string()).unwrap_or_default();
        // By name, as DBeaver's navigator shows them (the file's keys are ids)
        let mut list: Vec<&Value> = conns.values().take(MAX_SOURCES - out.len().min(MAX_SOURCES)).collect();
        list.sort_by_key(|c| s(c, "name").to_lowercase());
        out.extend(list.into_iter().map(|c| map_dbeaver(c, &project)));
    }
    Ok(out)
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).map(str::trim).unwrap_or_default()
}

/// A port written as "5432" or 5432
fn port_of(v: &Value) -> Option<u16> {
    match v {
        Value::String(s) => s.trim().parse().ok(),
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        _ => None,
    }
}

fn map_dbeaver(c: &Value, project: &str) -> Foreign {
    let name = s(c, "name").to_string();
    let (provider, driver) = (s(c, "provider").to_ascii_lowercase(), s(c, "driver").to_ascii_lowercase());
    let empty = Value::Null;
    let cfg = c.get("configuration").unwrap_or(&empty);
    let kind = match provider.as_str() {
        "postgresql" => Some(DbKind::Postgres),
        "mysql" => Some(DbKind::Mysql),
        "oracle" => Some(DbKind::Oracle),
        "sqlserver" | "mssql" => Some(DbKind::Mssql),
        "sqlite" => Some(DbKind::Sqlite),
        "clickhouse" => Some(DbKind::Clickhouse),
        "duckdb" => Some(DbKind::Duckdb),
        "mongodb" => Some(DbKind::Mongodb),
        "redis" => Some(DbKind::Redis),
        // Generic JDBC: by the driver (SQLite, DuckDB, ClickHouse ship as generic drivers)
        "generic" if driver.contains("sqlite") => Some(DbKind::Sqlite),
        "generic" if driver.contains("duckdb") => Some(DbKind::Duckdb),
        "generic" if driver.contains("clickhouse") => Some(DbKind::Clickhouse),
        _ => parse_url(s(cfg, "url")).map(|t| t.kind),
    };
    let Some(kind) = kind else {
        let label = if provider == "generic" && (driver.contains("csv") || driver.contains("file")) { "file drivers".to_string() } else { unsupported_label(if provider == "generic" { &driver } else { &provider }, s(cfg, "url")) };
        let reason = if label == "file drivers" { "file drivers aren't supported".into() } else { format!("{label} isn't supported") };
        return Foreign { name, kind_label: if label == "file drivers" { "Flat files (CSV)".into() } else { label }, what: Err(reason) };
    };
    // host / port / database as DBeaver shows them; the URL only when they're missing (a URL-only connection)
    let from_url = parse_url(s(cfg, "url")).filter(|t| t.kind == kind);
    let mut t = Target { kind, host: s(cfg, "host").to_string(), port: cfg.get("port").and_then(port_of), database: s(cfg, "database").to_string(), sid: false };
    if let Some(u) = &from_url {
        if t.host.is_empty() && !kind.is_file() {
            (t.host, t.port) = (u.host.clone(), t.port.or(u.port));
        }
        if t.database.is_empty() {
            t.database = u.database.clone();
        }
        t.sid = u.sid;
    }
    if kind == DbKind::Oracle && cfg.get("provider-properties").map(|p| s(p, "@dbeaver-sid-service@")) == Some("SID") {
        t.sid = true;
    }
    let mut m = new_mapped(&name, t);
    // DBeaver keeps the user name with the password (credentials-config.json, which is never read); an older or plain
    // configuration has it here
    m.config.user = s(cfg, "user").to_string();
    if m.config.user.is_empty() && !kind.is_file() && !matches!(kind, DbKind::Redis | DbKind::Mongodb) {
        m.warn.push("no user name in the file (DBeaver keeps it with the password)".into());
    }
    if c.get("read-only").and_then(Value::as_bool) == Some(true) {
        m.config.read_only = true;
    }
    if s(cfg, "type") == "prod" {
        m.config.options.confirm_changes = true;
        m.mapped.push("Production".into());
    }
    let expand = |p: &str| p.replace("${project.path}", project);
    // The tunnel first, then SSL (the note reads in the order of the form)
    let mut handlers: Vec<(&String, &Value)> = cfg.get("handlers").and_then(Value::as_object).into_iter().flatten().collect();
    handlers.sort_by_key(|(id, _)| *id != "ssh_tunnel");
    for (id, h) in handlers {
        if h.get("enabled").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let props = h.get("properties").unwrap_or(&empty);
        if id == "ssh_tunnel" && !kind.is_file() {
            let auth_type = s(props, "authType");
            let key_path = expand(s(props, "keyPath"));
            let user = [s(h, "user"), s(props, "user")].into_iter().find(|u| !u.is_empty()).unwrap_or_default().to_string();
            if user.is_empty() {
                m.warn.push("no SSH user name in the file".into());
            }
            if auth_type == "AGENT" {
                m.warn.push("SSH key from the agent: choose a key file".into());
            }
            let auth = if auth_type == "PASSWORD" { SshAuth::Password } else { SshAuth::Key };
            set_ssh(&mut m, SshConfig { enabled: true, host: s(props, "host").to_string(), port: props.get("port").and_then(port_of).unwrap_or(22), user, auth, key_path });
        } else if id.ends_with("ssl") {
            let p = |keys: &[&str]| keys.iter().map(|k| s(props, k)).find(|v| !v.is_empty()).unwrap_or_default().to_string();
            let mode = p(&["sslMode", "ssl.sslMode", "ssl.mode"]).to_ascii_lowercase();
            let mode = match mode.as_str() {
                "disable" => continue,
                "verify-full" | "verify_identity" => "verify-full",
                "verify-ca" => "verify-ca",
                // MySQL: "Verify server certificate" checks the chain
                _ if s(props, "ssl.verify.server") == "true" => "verify-ca",
                _ => "require",
            };
            set_ssl(&mut m, mode, expand(&p(&["ssl.ca.cert", "rootCert", "sslRootCert"])), expand(&p(&["ssl.client.cert", "clientCert"])), expand(&p(&["ssl.client.key", "clientKey"])));
        }
    }
    let kind_label = if kind == DbKind::Mysql && driver.contains("maria") { "MariaDB".into() } else { kind_name(kind, "") };
    Foreign { name, kind_label, what: Ok(finish(m)) }
}

// ───────── Mapping helpers (both formats) ─────────

/// Where a JDBC URL points
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub kind: DbKind,
    pub host: String,
    pub port: Option<u16>,
    pub database: String,
    /// Oracle host:port:SID (Qurry connects by service name)
    pub sid: bool,
}

fn default_port(k: DbKind) -> u16 {
    match k {
        DbKind::Postgres => 5432,
        DbKind::Mysql => 3306,
        DbKind::Redis => 6379,
        DbKind::Mssql => 1433,
        DbKind::Clickhouse => 8123,
        DbKind::Mongodb => 27017,
        DbKind::Oracle => 1521,
        DbKind::Qdrant => 6333,
        DbKind::Chroma => 8000,
        DbKind::Milvus => 19530,
        DbKind::Sqlite | DbKind::Duckdb => 0,
    }
}

/// host[:port] of the first host in a list ("h1:5432,h2:5432", "[fd00::1]:5433")
fn host_port(auth: &str) -> (String, Option<u16>) {
    let first = auth.split(',').next().unwrap_or_default();
    let first = first.rsplit_once('@').map(|x| x.1).unwrap_or(first);
    if let Some(rest) = first.strip_prefix('[')
        && let Some((h, after)) = rest.split_once(']')
    {
        return (h.to_string(), after.strip_prefix(':').and_then(|p| p.parse().ok()));
    }
    match first.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h.to_string(), p.parse().ok()),
        _ => (first.to_string(), None),
    }
}

/// A JDBC (or MongoDB) URL of a kind Qurry has. Query parameters are dropped (they may hold a password)
pub(crate) fn parse_url(url: &str) -> Option<Target> {
    let u = url.trim();
    let rest = u.strip_prefix("jdbc:").unwrap_or(u);
    let lower = rest.to_ascii_lowercase();
    let net = |kind: DbKind, after: &str| -> Target {
        let (auth, path) = after.split_once('/').unwrap_or((after, ""));
        let auth = auth.split([';', '?']).next().unwrap_or_default();
        let (host, port) = host_port(auth);
        let database = path.split(['?', ';', '#']).next().unwrap_or_default().to_string();
        Target { kind, host, port, database, sid: false }
    };
    let after = |p: &str| rest.get(p.len()..).unwrap_or_default();
    for (p, kind) in [("postgresql://", DbKind::Postgres), ("mysql://", DbKind::Mysql), ("mariadb://", DbKind::Mysql), ("clickhouse://", DbKind::Clickhouse), ("ch://", DbKind::Clickhouse), ("redis://", DbKind::Redis), ("rediss://", DbKind::Redis), ("mongodb://", DbKind::Mongodb)] {
        if lower.starts_with(p) {
            return Some(net(kind, after(p)));
        }
    }
    if lower.starts_with("mongodb+srv://") {
        // SRV: the whole URI is the host (Qurry's MongoDB host takes a URI), without user info or options
        let base = crate::store::without_userinfo(rest.split('?').next().unwrap_or(rest));
        return Some(Target { kind: DbKind::Mongodb, host: base, port: None, database: String::new(), sid: false });
    }
    for p in ["sqlserver://", "jtds:sqlserver://"] {
        if lower.starts_with(p) {
            let a = after(p);
            let mut t = net(DbKind::Mssql, a);
            // ;databaseName=x;… after the host
            for kv in a.split(';').skip(1) {
                if let Some((k, v)) = kv.split_once('=')
                    && (k.eq_ignore_ascii_case("databaseName") || k.eq_ignore_ascii_case("database"))
                {
                    t.database = v.to_string();
                }
            }
            return Some(t);
        }
    }
    if let Some(a) = lower.strip_prefix("oracle:thin:").map(|_| after("oracle:thin:")) {
        let a = a.split_once('@').map(|x| x.1).unwrap_or(a);
        if a.starts_with('(') {
            return None; // a TNS descriptor
        }
        let a = a.trim_start_matches("//");
        let (hp, svc) = a.split_once('/').unwrap_or((a, ""));
        // host:port:SID
        let parts: Vec<&str> = hp.split(':').collect();
        let (host, port, sid) = match parts.as_slice() {
            [h, p, s] => (h.to_string(), p.parse().ok(), Some(s.to_string())),
            [h, p] => (h.to_string(), p.parse().ok(), None),
            _ => (hp.to_string(), None, None),
        };
        let database = sid.clone().unwrap_or_else(|| svc.split('?').next().unwrap_or_default().to_string());
        return Some(Target { kind: DbKind::Oracle, host, port, database, sid: sid.is_some() });
    }
    for (p, kind) in [("sqlite:", DbKind::Sqlite), ("duckdb:", DbKind::Duckdb)] {
        if lower.starts_with(p) {
            let path = after(p).split('?').next().unwrap_or_default().to_string();
            return Some(Target { kind, host: String::new(), port: None, database: path, sid: false });
        }
    }
    None
}

pub(crate) fn kind_name(k: DbKind, driver: &str) -> String {
    if k == DbKind::Mysql && driver.contains("maria") {
        return "MariaDB".into();
    }
    match k {
        DbKind::Postgres => "PostgreSQL",
        DbKind::Mysql => "MySQL",
        DbKind::Redis => "Redis",
        DbKind::Sqlite => "SQLite",
        DbKind::Mssql => "SQL Server",
        DbKind::Clickhouse => "ClickHouse",
        DbKind::Duckdb => "DuckDB",
        DbKind::Mongodb => "MongoDB",
        DbKind::Oracle => "Oracle",
        DbKind::Qdrant => "Qdrant",
        DbKind::Chroma => "Chroma",
        DbKind::Milvus => "Milvus",
    }
    .into()
}

/// The name of a kind Qurry doesn't have, from the driver id or the URL
fn unsupported_label(driver: &str, url: &str) -> String {
    let id = if driver.is_empty() { url.trim_start_matches("jdbc:").split([':', '/']).next().unwrap_or_default() } else { driver };
    let id = id.split(['.', '_', '-']).next().unwrap_or(id).to_ascii_lowercase();
    match id.as_str() {
        "snowflake" => "Snowflake",
        "bigquery" => "BigQuery",
        "redshift" => "Redshift",
        "db2" => "Db2",
        "h2" => "H2",
        "hsqldb" => "HSQLDB",
        "derby" => "Derby",
        "cassandra" => "Cassandra",
        "hive" => "Hive",
        "vertica" => "Vertica",
        "exasol" => "Exasol",
        "sybase" => "Sybase",
        "athena" => "Athena",
        "databricks" => "Databricks",
        "trino" => "Trino",
        "presto" => "Presto",
        "firebird" => "Firebird",
        "informix" => "Informix",
        "teradata" => "Teradata",
        "sap" | "hana" => "SAP HANA",
        "" => "This kind",
        other => return other.to_string(),
    }
    .into()
}

fn new_mapped(name: &str, t: Target) -> Mapped {
    let mut warn = vec![];
    if t.sid {
        warn.push("Oracle SID used as the service name".into());
    }
    let config = ConnectionConfig {
        id: String::new(),
        name: name.to_string(),
        kind: t.kind,
        host: t.host,
        port: t.port.unwrap_or(default_port(t.kind)),
        user: String::new(),
        database: t.database,
        scope: Default::default(),
        password_saved: false,
        ssh_secret_saved: false,
        ssh: None,
        color: None,
        read_only: false,
        options: crate::store::ConnOptions { mcp: Some("off".into()), ..Default::default() },
        ssl: None,
        redis: None,
        ssm: None,
    };
    Mapped { config, mapped: vec![], warn, no_password: false }
}

fn set_ssh(m: &mut Mapped, ssh: SshConfig) {
    let what = match ssh.auth {
        SshAuth::Key if !ssh.key_path.is_empty() => format!("key {}", ssh.key_path),
        SshAuth::Key => "key".to_string(),
        SshAuth::Password => "password".to_string(),
    };
    let port = if ssh.port == 22 { String::new() } else { format!(":{}", ssh.port) };
    let user = if ssh.user.is_empty() { String::new() } else { format!("{}@", ssh.user) };
    m.mapped.push(format!("SSH {user}{}{port} ({what})", ssh.host));
    m.config.ssh = Some(ssh);
}

fn set_ssl(m: &mut Mapped, mode: &str, ca_file: String, cert_file: String, key_file: String) {
    let mut what = format!("SSL {mode}");
    if !ca_file.is_empty() {
        what += &format!(" (CA {ca_file})");
    }
    m.mapped.push(what);
    if mode == "require" {
        m.warn.push("SSL without certificate checks".into());
    }
    m.config.ssl = Some(SslConfig { enabled: true, mode: mode.into(), ca_file, cert_file, key_file });
}

/// Without control characters: a name or host from a file can't move the cursor of a terminal, or hide in a line
fn clean(s: &mut String) {
    s.retain(|c| !c.is_control());
}

fn finish(mut m: Mapped) -> Mapped {
    if m.config.read_only {
        m.mapped.push("Read-only".into());
    }
    let c = &mut m.config;
    for s in [&mut c.name, &mut c.host, &mut c.user, &mut c.database] {
        clean(s);
    }
    if let Some(ssh) = &mut c.ssh {
        for s in [&mut ssh.host, &mut ssh.user, &mut ssh.key_path] {
            clean(s);
        }
    }
    if let Some(ssl) = &mut c.ssl {
        for s in [&mut ssl.ca_file, &mut ssl.cert_file, &mut ssl.key_file] {
            clean(s);
        }
    }
    for s in m.mapped.iter_mut().chain(m.warn.iter_mut()) {
        clean(s);
    }
    m
}

/// "app@orders-db.internal:5432/orders", a file path, a MongoDB URI
pub(crate) fn server(c: &ConnectionConfig) -> String {
    if c.kind.is_file() {
        return c.database.clone();
    }
    let host = crate::store::without_userinfo(&c.host);
    if host.contains("://") {
        return host;
    }
    let user = if c.user.is_empty() { String::new() } else { format!("{}@", c.user) };
    let host = if host.contains(':') { format!("[{host}]") } else { host };
    let db = if c.database.is_empty() { String::new() } else { format!("/{}", c.database) };
    format!("{user}{host}:{}{db}", c.port)
}

/// The same source as one Qurry has: kind, server, user and database
pub(crate) fn same_source(a: &ConnectionConfig, b: &ConnectionConfig) -> bool {
    let key = |c: &ConnectionConfig| (c.kind, c.host.trim().to_lowercase(), if c.kind.is_file() { 0 } else { c.port }, c.user.trim().to_string(), c.database.trim().to_string());
    key(a) == key(b)
}

/// The plan for data sources read from DataGrip or DBeaver, against what Qurry has
pub(crate) fn foreign_plan(from: &str, path: &str, existing: &[ConnectionConfig], sources: Vec<Foreign>) -> ImportPlan {
    let (mut rows, mut configs) = (vec![], vec![]);
    let mut taken: Vec<ConnectionConfig> = vec![];
    for mut f in sources {
        clean(&mut f.name);
        clean(&mut f.kind_label);
        let skip = |reason: String| ImportRow { text: format!("− skip \"{}\" ({})", f.name, f.kind_label), note: reason, value: "skipped".into(), tone: None, checkable: false };
        match f.what {
            Err(reason) => {
                rows.push(skip(reason));
                configs.push(None);
            }
            Ok(m) => {
                if existing.iter().any(|e| same_source(e, &m.config)) {
                    rows.push(skip("already in Qurry (same server, user and database)".into()));
                    configs.push(None);
                    continue;
                }
                if let Some(t) = taken.iter().find(|t| same_source(t, &m.config)) {
                    rows.push(skip(format!("the same server, user and database as \"{}\"", t.name)));
                    configs.push(None);
                    continue;
                }
                let note = std::iter::once(f.kind_label.clone()).chain(m.mapped.iter().cloned()).collect::<Vec<_>>().join(" · ");
                let (value, tone) = if !m.warn.is_empty() {
                    (m.warn.join("; "), Some("warn".to_string()))
                } else if m.no_password {
                    ("no password".into(), None)
                } else {
                    ("password on connect".into(), None)
                };
                rows.push(ImportRow { text: format!("+ add \"{}\" ({})", f.name, server(&m.config)), note, value, tone, checkable: true });
                taken.push(m.config.clone());
                configs.push(Some(m.config));
            }
        }
    }
    // What is added first, then what is skipped (each in the file's order)
    let (a, b): (Vec<_>, Vec<_>) = rows.into_iter().zip(configs).partition(|(_, c)| c.is_some());
    let (rows, configs): (Vec<ImportRow>, Vec<Option<ConnectionConfig>>) = a.into_iter().chain(b).unzip();
    let add = configs.iter().filter(|c| c.is_some()).count();
    let skipped = rows.len() - add;
    let message = if rows.is_empty() {
        format!("Nothing to import: {from} has no data sources.\n{path}")
    } else if add == 0 {
        format!("Nothing to import from {from}: its {} data source{} {} skipped.\n{path}", rows.len(), if rows.len() == 1 { "" } else { "s" }, if rows.len() == 1 { "is" } else { "are" })
    } else {
        format!("Import these data sources from {from}?\n{path}\n\n{add} to add, {skipped} skipped. {NOTE}")
    };
    ImportPlan { title: TITLE.into(), message, rows, importable: add, configs }
}

// ───────── Locations on this computer ─────────

/// The DataGrip / IntelliJ and DBeaver locations with data sources, newest IDE version first, then DBeaver.
/// Only reads; a location that can't be read or has no data sources isn't listed
pub fn locations(home: &Path) -> Vec<Location> {
    let mut out: Vec<Location> = vec![];
    let add = |label: String, dir: PathBuf, out: &mut Vec<Location>| {
        let canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if out.iter().any(|l| Path::new(&l.path).canonicalize().ok().as_ref() == Some(&canon)) {
            return;
        }
        let count = match read(&dir, home) {
            Ok(Read::Foreign(v)) => v.len(),
            _ => 0,
        };
        if count > 0 {
            out.push(Location { label, shown: shown(&dir, home), path: dir.display().to_string(), count });
        }
    };
    let project_name = |idea: &Path| idea.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or_default().to_string();
    for (cfg, product, version) in ide_configs(home) {
        let pname = product_name(&product);
        let options = cfg.join("options");
        if options.join("dataSources.xml").is_file() {
            add(format!("{pname} {version} · global"), options.clone(), &mut out);
        }
        // DataGrip's own projects, then the IDE's recent projects
        let mut ideas: Vec<PathBuf> = subdirs(&cfg.join("projects")).into_iter().map(|p| p.join(".idea")).collect();
        let recent = options.join("recentProjects.xml");
        if recent.is_file()
            && let Ok(root) = read_xml(&recent, home)
        {
            let mut entries = vec![];
            root.all("entry", &mut entries);
            let mut opts = vec![];
            root.all("option", &mut opts);
            let paths = entries.iter().filter_map(|e| e.attr("key")).chain(opts.iter().filter(|o| o.attr("name").is_none()).filter_map(|o| o.attr("value")));
            for p in paths.take(MAX_ENTRIES) {
                let p = PathBuf::from(p.replace("$USER_HOME$", &home.display().to_string()));
                // Only absolute paths, and only the project's own .idea is opened
                if p.is_absolute() && !p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    ideas.push(p.join(".idea"));
                }
            }
        }
        for idea in ideas.into_iter().filter(|i| i.join("dataSources.xml").is_file()) {
            add(format!("{pname} · project {}", project_name(&idea)), idea, &mut out);
        }
    }
    for p in subdirs(&home.join("DataGripProjects")) {
        let idea = p.join(".idea");
        if idea.join("dataSources.xml").is_file() {
            add(format!("DataGrip · project {}", project_name(&idea)), idea, &mut out);
        }
    }
    let dbeaver = [
        ("DBeaver", home.join("Library/DBeaverData/workspace6")),
        ("DBeaver", home.join(".local/share/DBeaverData/workspace6")),
        ("DBeaver (Flatpak)", home.join(".var/app/io.dbeaver.DBeaverCommunity/data/DBeaverData/workspace6")),
        ("DBeaver (Snap)", home.join("snap/dbeaver-ce/current/.local/share/DBeaverData/workspace6")),
    ];
    for (tool, ws) in dbeaver {
        for p in subdirs(&ws) {
            let dir = p.join(".dbeaver");
            if !dbeaver_files(&dir).is_empty() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                add(format!("{tool} · workspace6 {name}"), dir, &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jdbc_urls() {
        let t = parse_url("jdbc:postgresql://db.internal:6432/shop?password=x").unwrap();
        assert_eq!((t.kind, t.host.as_str(), t.port, t.database.as_str()), (DbKind::Postgres, "db.internal", Some(6432), "shop"));
        let t = parse_url("jdbc:postgresql://h1:5432,h2:5432/app").unwrap();
        assert_eq!((t.host.as_str(), t.port), ("h1", Some(5432)));
        let t = parse_url("jdbc:postgresql://[fd00::15]:5433/ops").unwrap();
        assert_eq!((t.host.as_str(), t.port), ("fd00::15", Some(5433)));
        let t = parse_url("jdbc:mysql://legacy.internal/shop_old").unwrap();
        assert_eq!((t.kind, t.port, t.database.as_str()), (DbKind::Mysql, None, "shop_old"));
        let t = parse_url("jdbc:sqlserver://mssql.internal:1433;databaseName=legacy;encrypt=true").unwrap();
        assert_eq!((t.kind, t.host.as_str(), t.port, t.database.as_str()), (DbKind::Mssql, "mssql.internal", Some(1433), "legacy"));
        let t = parse_url("jdbc:oracle:thin:@//ora.internal:1521/ORCLPDB1").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.database.as_str(), t.sid), ("ora.internal", Some(1521), "ORCLPDB1", false));
        let t = parse_url("jdbc:oracle:thin:@ora.internal:1521:ORCL").unwrap();
        assert_eq!((t.database.as_str(), t.sid), ("ORCL", true));
        assert!(parse_url("jdbc:oracle:thin:@(DESCRIPTION=(ADDRESS=(HOST=x)))").is_none());
        let t = parse_url("jdbc:sqlite:/var/data/app.db").unwrap();
        assert_eq!((t.kind, t.database.as_str()), (DbKind::Sqlite, "/var/data/app.db"));
        let t = parse_url("mongodb+srv://u:secret@logs.example.net/?retryWrites=true").unwrap();
        assert_eq!(t.host, "mongodb+srv://logs.example.net/");
        let t = parse_url("jdbc:redis://localhost:6379/2").unwrap();
        assert_eq!((t.kind, t.database.as_str()), (DbKind::Redis, "2"));
        assert!(parse_url("jdbc:snowflake://acme.example/").is_none());
    }

    #[test]
    fn xml_errors_have_a_position() {
        let e = parse_xml("<a>\n  <b>\n</a>").unwrap_err();
        assert!(e.starts_with("line 3, column "), "{e}");
        let e = parse_xml("<a>\n  <b>").unwrap_err();
        assert!(e.contains("<b> isn't closed"), "{e}");
        let e = parse_xml("<a>&nope;</a>").unwrap_err();
        assert!(e.contains("&nope;"), "{e}");
        let ok = parse_xml("<a x=\"&quot;1&quot;\">&lt;t&#33;</a>").unwrap();
        assert_eq!((ok.kids[0].attr("x"), ok.kids[0].text.as_str()), (Some("\"1\""), "<t!"));
    }

    /// A name or host with control characters (a terminal escape) loses them
    #[test]
    fn control_characters_are_dropped() {
        let ds = parse_xml("<data-source name=\"evil&#27;[2J\" uuid=\"1\"><jdbc-url>jdbc:postgresql://h&#27;[31m.internal/db</jdbc-url></data-source>").unwrap();
        let f = map_jetbrains(&ds.kids[0], None, &[], None);
        let plan = foreign_plan("x", "y", &[], vec![f]);
        assert_eq!(plan.rows[0].text, "+ add \"evil[2J\" (h[31m.internal:5432/db)");
        assert!(plan.configs[0].as_ref().is_some_and(|c| !c.name.contains('\x1b') && !c.host.contains('\x1b')));
    }

    #[test]
    fn ide_dirs() {
        assert_eq!(ide_dir("DataGrip2025.2"), Some(("DataGrip".into(), "2025.2".into())));
        assert_eq!(ide_dir("IntelliJIdea2024.3"), Some(("IntelliJIdea".into(), "2024.3".into())));
        assert_eq!(ide_dir("consentOptions"), None);
    }
}
