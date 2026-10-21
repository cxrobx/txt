//! Recently closed notes, newest first: tabs closed, windows closed, or a tab
//! switched to another note. One list for the whole app (`closed.json` in the
//! support folder), so every window's ⌘P list and ⌘⇧T agree, and it survives
//! a restart. Ids only: titles come from the note list, so a deleted note
//! just drops out.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

const KEEP: usize = 20;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Closed {
    pub id: String,
    /// Unix seconds.
    pub closed_at: u64,
}

pub struct ClosedList {
    path: PathBuf,
    items: Mutex<Vec<Closed>>,
}

impl ClosedList {
    pub fn open(support: &std::path::Path) -> Self {
        let path = support.join("closed.json");
        let items = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, items: Mutex::new(items) }
    }

    pub fn list(&self) -> Vec<Closed> {
        self.items.lock().unwrap().clone()
    }

    /// Closing again moves a note to the top; the list keeps the newest 20.
    pub fn record(&self, ids: &[String], now: u64) {
        let mut items = self.items.lock().unwrap();
        for id in ids {
            items.retain(|c| &c.id != id);
            items.insert(0, Closed { id: id.clone(), closed_at: now });
        }
        items.truncate(KEEP);
        self.save(&items);
    }

    /// A note that's open again isn't "closed" any more.
    pub fn forget(&self, id: &str) -> bool {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|c| c.id != id);
        let changed = items.len() != before;
        if changed {
            self.save(&items);
        }
        changed
    }

    fn save(&self, items: &[Closed]) {
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec(items).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_no_repeats_capped_and_kept_across_restarts() {
        let dir = std::env::temp_dir().join(format!("txt-closed-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = ClosedList::open(&dir);
        list.record(&["a".into(), "b".into()], 1);
        list.record(&["a".into()], 2);
        assert_eq!(list.list().iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(list.forget("a"));
        assert!(!list.forget("a"));
        list.record(&(0..30).map(|i| i.to_string()).collect::<Vec<_>>(), 3);
        assert_eq!(list.list().len(), KEEP);
        assert_eq!(ClosedList::open(&dir).list(), list.list());
    }
}
