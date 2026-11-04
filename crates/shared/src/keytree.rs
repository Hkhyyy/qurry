//! Redis keys as folders split on ":" (like Medis).
use serde::{Deserialize, Serialize};

/// Where the TUI and the GUI order folders differently (D2; the keyTree row of crates/shared/tests/DIVERGENCES.md)
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// Folder names in UTF-16 code unit order, as JavaScript sorts strings (the GUI). Otherwise in code point order (the TUI):
    /// they differ only past U+FFFF, e.g. 😀 sorts before ！ in UTF-16
    pub utf16_order: bool,
}

/// A folder (path = "dev:sessions", count = keys below it) or a key, by its index in the input
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KeyNode {
    Folder { name: String, path: String, count: usize, children: Vec<KeyNode> },
    Key { item: usize },
}

/// Folder levels at most: a key with more ":" sits in the deepest folder (a key of thousands of ":" would overflow the stack)
pub const MAX_DEPTH: usize = 64;

/// Folders first (by name), then keys in the order given. Keys keep their full names
pub fn key_tree(keys: &[String], opts: Options) -> Vec<KeyNode> {
    level(keys, (0..keys.len()).collect(), 0, "", opts)
}

fn level(keys: &[String], items: Vec<usize>, depth: usize, prefix: &str, opts: Options) -> Vec<KeyNode> {
    let mut folders: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
    let mut leaves = vec![];
    for i in items {
        match keys[i].split(':').nth(depth) {
            Some(seg) if depth < MAX_DEPTH && keys[i].split(':').nth(depth + 1).is_some() => folders.entry(seg).or_default().push(i),
            _ => leaves.push(KeyNode::Key { item: i }),
        }
    }
    let mut folders: Vec<_> = folders.into_iter().collect();
    if opts.utf16_order {
        folders.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    }
    let mut out: Vec<KeyNode> = folders
        .into_iter()
        .map(|(seg, kids)| {
            let path = if prefix.is_empty() { seg.to_string() } else { format!("{prefix}:{seg}") };
            let children = level(keys, kids.clone(), depth + 1, &path, opts);
            KeyNode::Folder { name: seg.to_string(), path, count: kids.len(), children }
        })
        .collect();
    out.extend(leaves);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_stop_at_max_depth() {
        let keys = ["a:".repeat(10_000) + "x"];
        let (mut nodes, mut depth) = (key_tree(&keys, Options::default()), 0);
        while let [KeyNode::Folder { children, .. }] = nodes.as_slice() {
            nodes = children.clone();
            depth += 1;
        }
        assert_eq!((depth, nodes), (MAX_DEPTH, vec![KeyNode::Key { item: 0 }]));
    }
}
