//! Workspace list (name · explorer Data Sources) shared by the GUI and TUI: config/workspaces.json (0600)
//! Tabs · splits are per app (workspace-gui.json / workspace-tui.json, matched by workspace id).
//! So they don't overwrite each other when both run, apps don't write the whole list; only the changes (prev → next) are applied to the file
use crate::error::AppResult;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub conn_ids: Vec<String>,
}

/// Result of reconciling the app's list with the shared list
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adopted {
    /// List to use from now on (the shared list)
    pub list: Vec<WsInfo>,
    /// App-side id → shared list id (on first sync, entries with the same name are paired)
    pub renamed: Vec<(String, String)>,
}

fn path(store: &Store) -> std::path::PathBuf {
    store.file("workspaces.json")
}

fn read(store: &Store) -> Option<Vec<WsInfo>> {
    let s = fs::read_to_string(path(store)).ok()?;
    serde_json::from_str::<Vec<WsInfo>>(&s).ok().filter(|l| !l.is_empty())
}

fn write(store: &Store, list: &[WsInfo]) -> AppResult<()> {
    crate::secrets::write_private(&path(store), serde_json::to_string_pretty(list)?.as_bytes())
}

/// Apply the prev → next diff to list (create · delete · rename · add/remove Data Sources). Order follows next
pub fn apply_diff(list: &mut Vec<WsInfo>, prev: &[WsInfo], next: &[WsInfo]) {
    let find = |l: &[WsInfo], id: &str| l.iter().position(|w| w.id == id);
    for p in prev {
        if find(next, &p.id).is_none()
            && let Some(i) = find(list, &p.id)
        {
            list.remove(i);
        }
    }
    for n in next {
        let Some(p) = prev.iter().find(|w| w.id == n.id) else {
            if find(list, &n.id).is_none() {
                list.push(n.clone());
            }
            continue;
        };
        let Some(i) = find(list, &n.id) else { continue };
        if n.name != p.name {
            list[i].name = n.name.clone();
        }
        for c in n.conn_ids.iter().filter(|c| !p.conn_ids.contains(c)) {
            if !list[i].conn_ids.contains(c) {
                list[i].conn_ids.push(c.clone());
            }
        }
        list[i].conn_ids.retain(|c| n.conn_ids.contains(c) || !p.conn_ids.contains(c));
    }
    // If this app reordered, follow it (ones created by other apps go at the end)
    let order = |w: &WsInfo| next.iter().position(|n| n.id == w.id).unwrap_or(usize::MAX);
    if prev.iter().map(|w| &w.id).ne(next.iter().filter(|n| prev.iter().any(|p| p.id == n.id)).map(|w| &w.id)) {
        list.sort_by_key(order);
    }
}

/// On first launch: reconcile the app's list (local) with the shared list.
/// synced = the app has synced with the shared list before → entries missing from the shared list were deleted by another app, so drop them.
/// The first time (files before 0.2.10), pair entries with the same name and add the rest to the shared list
pub fn adopt(shared: Option<Vec<WsInfo>>, local: &[WsInfo], synced: bool) -> (Adopted, bool) {
    let Some(mut list) = shared else {
        return (Adopted { list: local.to_vec(), renamed: vec![] }, true);
    };
    let mut renamed = vec![];
    let mut changed = false;
    for l in local {
        if list.iter().any(|w| w.id == l.id) || synced {
            continue;
        }
        let claimed = |id: &str| local.iter().any(|x| x.id == id) || renamed.iter().any(|(_, s): &(String, String)| s == id);
        if let Some(w) = list.iter().find(|w| w.name.eq_ignore_ascii_case(&l.name) && !claimed(&w.id)) {
            renamed.push((l.id.clone(), w.id.clone()));
            // Keep Data Sources added only in this app visible too
            let id = w.id.clone();
            let w = list.iter_mut().find(|w| w.id == id).unwrap();
            for c in &l.conn_ids {
                if !w.conn_ids.contains(c) {
                    w.conn_ids.push(c.clone());
                    changed = true;
                }
            }
        } else {
            list.push(l.clone());
            changed = true;
        }
    }
    (Adopted { list, renamed }, changed)
}

impl Store {
    /// Shared workspace list (None if none yet)
    pub fn shared_workspaces(&self) -> Option<Vec<WsInfo>> {
        read(self)
    }

    /// On first launch, reconcile the app's list with the shared list (writing the file if needed) and return the result
    pub fn adopt_workspaces(&self, local: &[WsInfo], synced: bool) -> AppResult<Adopted> {
        let (a, write_back) = adopt(read(self), local, synced);
        if write_back {
            write(self, &a.list)?;
        }
        Ok(a)
    }

    /// Apply this app's changes (prev → next) to the shared list and return the resulting list (includes other apps' changes)
    pub fn sync_workspaces(&self, prev: &[WsInfo], next: &[WsInfo]) -> AppResult<Vec<WsInfo>> {
        let mut list = read(self).unwrap_or_else(|| prev.to_vec());
        apply_diff(&mut list, prev, next);
        if list.is_empty() {
            list = next.to_vec();
        }
        write(self, &list)?;
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: &str, name: &str, conns: &[&str]) -> WsInfo {
        WsInfo { id: id.into(), name: name.into(), conn_ids: conns.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn diff_keeps_the_other_apps_changes() {
        // Shared: another app created b and added c2 to a
        let mut list = vec![ws("a", "Default", &["c1", "c2"]), ws("b", "Reports", &[])];
        let prev = vec![ws("a", "Default", &["c1"])];
        // This app: renamed a, removed c1, added c3, created n
        let next = vec![ws("a", "Main", &["c3"]), ws("n", "New", &["c1"])];
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list, vec![ws("a", "Main", &["c2", "c3"]), ws("b", "Reports", &[]), ws("n", "New", &["c1"])]);
        // Delete
        let prev = list.clone();
        let next: Vec<WsInfo> = list.iter().filter(|w| w.id != "b").cloned().collect();
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["a", "n"]);
        // Reorder
        let prev = list.clone();
        let next = vec![list[1].clone(), list[0].clone()];
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["n", "a"]);
    }

    #[test]
    fn adopt_first_time_and_after() {
        // With no shared list, this app's list becomes the shared list
        let local = vec![ws("ws1", "Default", &["c1"]), ws("w2", "Ops", &["c2"])];
        let (a, write) = adopt(None, &local, false);
        assert!(write && a.list == local && a.renamed.is_empty());
        // Another app created it first: pair by name (merging Data Sources), add the missing ones
        let shared = vec![ws("t1", "default", &["c3"]), ws("t2", "Logs", &[])];
        let (a, write) = adopt(Some(shared.clone()), &local, false);
        assert!(write);
        assert_eq!(a.renamed, vec![("ws1".to_string(), "t1".to_string())]);
        assert_eq!(a.list, vec![ws("t1", "default", &["c3", "c1"]), ws("t2", "Logs", &[]), ws("w2", "Ops", &["c2"])]);
        // If synced before: entries missing from the shared list were deleted
        let (a, write) = adopt(Some(shared.clone()), &[ws("t1", "default", &[]), ws("gone", "Old", &[])], true);
        assert!(!write && a.list == shared && a.renamed.is_empty());
    }

    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("qurry-ws-{}", uuid::Uuid::new_v4()));
        let store = Store::new(dir.clone());
        assert!(store.shared_workspaces().is_none());
        let gui = vec![ws("g1", "Default", &["c1"])];
        assert_eq!(store.adopt_workspaces(&gui, false).unwrap().list, gui);
        // TUI launched for the first time (old file: names only)
        let tui = store.adopt_workspaces(&[ws("tmp", "Default", &["c2"]), ws("tmp2", "Ops", &[])], false).unwrap();
        assert_eq!(tui.renamed, vec![("tmp".to_string(), "g1".to_string())]);
        // When the GUI renames, the TUI-side changes (Ops, c2) remain
        let after = store.sync_workspaces(&gui, &[ws("g1", "Main", &["c1"])]).unwrap();
        assert_eq!(after, vec![ws("g1", "Main", &["c1", "c2"]), ws("tmp2", "Ops", &[])]);
        assert_eq!(store.shared_workspaces().unwrap(), after);
        let _ = std::fs::remove_dir_all(dir);
    }
}
