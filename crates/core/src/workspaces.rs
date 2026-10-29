//! 워크스페이스 목록 (이름 · 탐색기의 Data Source) 을 GUI 와 TUI 가 같이 쓴다: config/workspaces.json (0600)
//! 탭 · 화면 분할은 앱마다 따로 (workspace-gui.json / workspace-tui.json, 워크스페이스 id 로 짝짓는다).
//! 둘이 같이 켜져 있어도 덮어쓰지 않게, 앱은 목록을 통째로 쓰지 않고 바뀐 것 (prev → next) 만 파일에 반영한다
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

/// 앱 쪽 목록을 공유 목록에 맞춘 결과
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adopted {
    /// 이제 쓸 목록 (공유 목록)
    pub list: Vec<WsInfo>,
    /// 앱 쪽 id → 공유 목록의 id (처음 맞출 때 이름이 같은 것끼리 묶는다)
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

/// prev → next 의 차이를 list 에 반영한다 (만들기 · 지우기 · 이름 · Data Source 넣고 빼기). 순서는 next 를 따른다
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
    // 이 앱에서 순서를 바꿨으면 따른다 (다른 앱이 만든 것은 뒤에)
    let order = |w: &WsInfo| next.iter().position(|n| n.id == w.id).unwrap_or(usize::MAX);
    if prev.iter().map(|w| &w.id).ne(next.iter().filter(|n| prev.iter().any(|p| p.id == n.id)).map(|w| &w.id)) {
        list.sort_by_key(order);
    }
}

/// 처음 켤 때: 앱 쪽 목록 (local) 을 공유 목록에 맞춘다.
/// synced = 앱이 전에 공유 목록과 맞춘 적이 있다 → 공유 목록에 없는 것은 다른 앱에서 지운 것이니 버린다.
/// 처음이면 (0.2.10 이전 파일) 이름이 같은 것끼리 묶고, 나머지는 공유 목록에 더한다
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
            // 이 앱에서만 넣어 둔 Data Source 도 그대로 보이게
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
    /// 공유 워크스페이스 목록 (아직 없으면 None)
    pub fn shared_workspaces(&self) -> Option<Vec<WsInfo>> {
        read(self)
    }

    /// 처음 켤 때 앱 쪽 목록을 공유 목록에 맞추고 (필요하면 파일에 쓴다) 결과를 돌려준다
    pub fn adopt_workspaces(&self, local: &[WsInfo], synced: bool) -> AppResult<Adopted> {
        let (a, write_back) = adopt(read(self), local, synced);
        if write_back {
            write(self, &a.list)?;
        }
        Ok(a)
    }

    /// 이 앱에서 바꾼 것 (prev → next) 을 공유 목록에 반영하고, 반영된 목록을 돌려준다 (다른 앱의 변경도 들어 있다)
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
        // 공유: 다른 앱이 b 를 만들고 a 에 c2 를 넣었다
        let mut list = vec![ws("a", "Default", &["c1", "c2"]), ws("b", "Reports", &[])];
        let prev = vec![ws("a", "Default", &["c1"])];
        // 이 앱: a 이름 바꾸고 c1 빼고 c3 넣고, n 을 만들었다
        let next = vec![ws("a", "Main", &["c3"]), ws("n", "New", &["c1"])];
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list, vec![ws("a", "Main", &["c2", "c3"]), ws("b", "Reports", &[]), ws("n", "New", &["c1"])]);
        // 지우기
        let prev = list.clone();
        let next: Vec<WsInfo> = list.iter().filter(|w| w.id != "b").cloned().collect();
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["a", "n"]);
        // 순서 바꾸기
        let prev = list.clone();
        let next = vec![list[1].clone(), list[0].clone()];
        apply_diff(&mut list, &prev, &next);
        assert_eq!(list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["n", "a"]);
    }

    #[test]
    fn adopt_first_time_and_after() {
        // 공유 목록이 없으면 이 앱 것이 공유 목록이 된다
        let local = vec![ws("ws1", "Default", &["c1"]), ws("w2", "Ops", &["c2"])];
        let (a, write) = adopt(None, &local, false);
        assert!(write && a.list == local && a.renamed.is_empty());
        // 다른 앱이 먼저 만들었다: 이름이 같은 것끼리 묶고 (Data Source 합침), 없는 것은 더한다
        let shared = vec![ws("t1", "default", &["c3"]), ws("t2", "Logs", &[])];
        let (a, write) = adopt(Some(shared.clone()), &local, false);
        assert!(write);
        assert_eq!(a.renamed, vec![("ws1".to_string(), "t1".to_string())]);
        assert_eq!(a.list, vec![ws("t1", "default", &["c3", "c1"]), ws("t2", "Logs", &[]), ws("w2", "Ops", &["c2"])]);
        // 전에 맞춘 적이 있으면: 공유 목록에 없는 것은 지워진 것
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
        // TUI 가 처음 켜졌다 (예전 파일: 이름만)
        let tui = store.adopt_workspaces(&[ws("tmp", "Default", &["c2"]), ws("tmp2", "Ops", &[])], false).unwrap();
        assert_eq!(tui.renamed, vec![("tmp".to_string(), "g1".to_string())]);
        // GUI 가 이름을 바꾸면 TUI 쪽 변경 (Ops, c2) 은 남는다
        let after = store.sync_workspaces(&gui, &[ws("g1", "Main", &["c1"])]).unwrap();
        assert_eq!(after, vec![ws("g1", "Main", &["c1", "c2"]), ws("tmp2", "Ops", &[])]);
        assert_eq!(store.shared_workspaces().unwrap(), after);
        let _ = std::fs::remove_dir_all(dir);
    }
}
