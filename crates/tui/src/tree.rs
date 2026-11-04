//! Database Explorer tree (lazy loading). Keys: "conn" / "conn|db" / "conn|db|schema" / "conn|db|schema|g:tables" / ".../o:name"
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Conn,
    Db,
    Schema,
    Group,
    /// table | view | matview | foreign | partitioned | sequence | function | procedure | key
    Object(String),
    Column { ty: String, pk: bool },
    Info,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub key: String,
    pub label: String,
    /// Dimmed on the right (type, count, etc.)
    pub hint: String,
    pub kind: NodeKind,
    pub conn: String,
    pub db: Option<String>,
    pub schema: Option<String>,
    pub name: Option<String>,
    pub expandable: bool,
}

impl Node {
    pub fn info(key: &str, conn: &str, text: impl Into<String>) -> Self {
        Node { key: format!("{key}|info"), label: text.into(), hint: String::new(), kind: NodeKind::Info, conn: conn.into(), db: None, schema: None, name: None, expandable: false }
    }
    pub fn is_relation(&self) -> bool {
        matches!(&self.kind, NodeKind::Object(k) if ["table", "view", "matview", "foreign", "partitioned"].contains(&k.as_str()))
    }
}

#[derive(Default)]
pub struct Tree {
    pub roots: Vec<Node>,
    pub children: HashMap<String, Vec<Node>>,
    pub expanded: HashSet<String>,
    pub loading: HashSet<String>,
    pub sel: usize,
    pub offset: usize,
    /// Speed search (/)
    pub filter: String,
    /// Horizontal scroll (long names)
    pub hoff: usize,
}

/// Case-insensitive contains check. ASCII names don't allocate a new string (called per node every time the tree is drawn)
fn contains_ci(hay: &str, lower_needle: &str) -> bool {
    if hay.is_ascii() && lower_needle.is_ascii() {
        let (h, n) = (hay.as_bytes(), lower_needle.as_bytes());
        return n.is_empty() || h.windows(n.len()).any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b));
    }
    hay.to_lowercase().contains(lower_needle)
}

impl Tree {
    /// The node's children are only the error its load ended with (load_children): expanding it again loads again
    pub fn failed(&self, key: &str) -> bool {
        self.children.get(key).is_some_and(|n| n.len() == 1 && n[0].kind == NodeKind::Info)
    }
    /// Lines visible on screen: (depth, node)
    pub fn visible(&self) -> Vec<(usize, &Node)> {
        let mut out = Vec::new();
        let f = self.filter.to_lowercase();
        fn walk<'a>(t: &'a Tree, nodes: &'a [Node], depth: usize, f: &str, out: &mut Vec<(usize, &'a Node)>) {
            for n in nodes {
                let open = t.expanded.contains(&n.key);
                let kids = if open { t.children.get(&n.key) } else { None };
                // Filter: visible if the name matches or an expanded child matches
                if !f.is_empty() && !matches!(n.kind, NodeKind::Conn | NodeKind::Db | NodeKind::Schema | NodeKind::Group) && !contains_ci(&n.label, f) {
                    continue;
                }
                out.push((depth, n));
                if let Some(k) = kids {
                    walk(t, k, depth + 1, f, out);
                } else if open && t.loading.contains(&n.key) {
                    // The renderer shows the loading indicator as a hint
                }
            }
        }
        walk(self, &self.roots, 0, &f, &mut out);
        out
    }

    pub fn selected(&self) -> Option<Node> {
        self.visible().get(self.sel).map(|(_, n)| (*n).clone())
    }

    pub fn select_key(&mut self, key: &str) -> bool {
        if let Some(i) = self.visible().iter().position(|(_, n)| n.key == key) {
            self.sel = i;
            true
        } else {
            false
        }
    }

    pub fn clamp(&mut self) {
        let n = self.visible().len();
        if n == 0 {
            self.sel = 0;
        } else if self.sel >= n {
            self.sel = n - 1;
        }
    }

    /// Disconnect·refresh: clear the cache below it
    pub fn forget(&mut self, prefix: &str) {
        self.children.retain(|k, _| !(k == prefix || k.starts_with(&format!("{prefix}|"))));
        self.loading.retain(|k| !(k == prefix || k.starts_with(&format!("{prefix}|"))));
    }

    pub fn parent_index(&self) -> Option<usize> {
        let vis = self.visible();
        let (depth, _) = *vis.get(self.sel)?;
        if depth == 0 {
            return None;
        }
        (0..self.sel).rev().find(|&i| vis[i].0 == depth - 1)
    }
}

/// Group Redis keys into folders split on ":" (like Medis; qurry_shared::keytree). Folders first (by name), then keys.
/// Nodes under a folder go in children. Key lines keep the full name
pub fn key_folders(children: &mut HashMap<String, Vec<Node>>, parent: &str, keys: Vec<Node>) -> Vec<Node> {
    use qurry_core::keytree::{self, KeyNode};
    /// The first key (in the order given) below a folder: the folder takes its connection, db and schema
    fn first(n: &KeyNode) -> usize {
        match n {
            KeyNode::Key { item } => *item,
            KeyNode::Folder { children, .. } => children.iter().map(first).min().unwrap_or(usize::MAX),
        }
    }
    fn nodes(children: &mut HashMap<String, Vec<Node>>, parent: &str, tree: Vec<KeyNode>, keys: &mut [Option<Node>]) -> Vec<Node> {
        let mut out = vec![];
        for n in tree {
            let f = first(&n);
            match n {
                KeyNode::Key { item } => out.extend(keys[item].take()),
                KeyNode::Folder { name, count, children: sub, .. } => {
                    let key = format!("{parent}|f:{name}");
                    let k = keys[f].as_ref().expect("a folder holds a key not yet placed");
                    let folder = Node { key: key.clone(), label: name, hint: count.to_string(), kind: NodeKind::Group, conn: k.conn.clone(), db: k.db.clone(), schema: k.schema.clone(), name: None, expandable: true };
                    let sub = nodes(children, &key, sub, keys);
                    children.insert(key, sub);
                    out.push(folder);
                }
            }
        }
        out
    }
    let names: Vec<String> = keys.iter().map(|n| n.label.clone()).collect();
    let tree = keytree::key_tree(&names, Default::default());
    let mut keys: Vec<Option<Node>> = keys.into_iter().map(Some).collect();
    nodes(children, parent, tree, &mut keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Node {
        Node { key: format!("g|o:{name}"), label: name.into(), hint: "zset".into(), kind: NodeKind::Object("key".into()), conn: "c".into(), db: Some("0".into()), schema: Some("0".into()), name: Some(name.into()), expandable: false }
    }

    /// dev:sessions:{1} → dev / sessions / dev:sessions:{1}. Folders show the count of keys below them
    #[test]
    fn redis_keys_are_grouped_by_colon() {
        let mut ch = HashMap::new();
        let keys = ["token", "dev:sessions:{2}", "prod:x", "dev:admin:{1}", "dev:sessions:{1}"].map(key).to_vec();
        let top = key_folders(&mut ch, "g", keys);
        let show = |v: &[Node]| v.iter().map(|n| format!("{}{} {}", if n.kind == NodeKind::Group { "/" } else { "" }, n.label, n.hint)).collect::<Vec<_>>();
        assert_eq!(show(&top), ["/dev 3", "/prod 1", "token zset"]);
        assert!(top[0].expandable && !top[2].expandable);
        let dev = &ch[&top[0].key];
        assert_eq!(show(dev), ["/admin 1", "/sessions 2"]);
        assert_eq!(show(&ch[&dev[1].key]), ["dev:sessions:{2} zset", "dev:sessions:{1} zset"]);
        // Info used to open the key stays as is
        assert_eq!(ch[&dev[1].key][0].name.as_deref(), Some("dev:sessions:{2}"));
    }

    #[test]
    fn contains_ignoring_case() {
        assert!(contains_ci("Customer_Company", "company"));
        assert!(contains_ci("orders", ""));
        assert!(!contains_ci("orders", "user"));
        assert!(contains_ci("결제_Payment", "payment"));
        assert!(contains_ci("ÄRGER", "ärg"));
    }
}
