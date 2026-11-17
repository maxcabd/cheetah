//! Saved app state, written when the app goes to the background and read at
//! launch, so reopening after iOS ends the app picks up where it left off.

use crate::kv::{KvResult, KvStore};
use crate::nightshade::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KEY: &str = "nightshade/state";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Saved {
    query: String,
    draft: String,
    /// `None` when there was no finished search.
    results: Option<SearchResponse>,
    selected: usize,
    focused: bool,
    center: Option<(f32, f32)>,
}

impl NightshadeApp {
    /// Keep state in `path` (a redb file of its own): restore it now, and
    /// save it whenever the app goes to the background.
    pub fn with_state_file(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(saved) = load(&path) {
            self.restore(saved);
        }
        self.state_file = Some(path);
        self
    }

    pub(crate) fn save_state(&self) {
        if let Some(path) = &self.state_file {
            if let Err(error) = save(path, &self.snapshot()) {
                eprintln!("nightshade: saving state failed: {error}");
            }
        }
    }

    fn snapshot(&self) -> Saved {
        Saved {
            query: self.query.clone(),
            draft: self.draft.clone(),
            results: self.response().cloned(),
            selected: self.selected,
            focused: self.focused,
            center: self.center,
        }
    }

    fn restore(&mut self, saved: Saved) {
        self.center = saved.center.or(self.center);
        match saved.results {
            Some(results) => {
                self.selected = saved.selected.min(results.places.len().saturating_sub(1));
                self.focused = saved.focused;
                self.query = saved.query;
                self.search = Resource::Ready(results);
            }
            // A search still running when the app was closed runs again.
            None if !saved.query.is_empty() => self.run_search(&saved.query),
            None => {}
        }
        self.draft = saved.draft;
    }
}

// Opened per call: the file is small, and redb locks it while open, which a
// dev server reconnecting before the old session closes would trip over.
fn load(path: &Path) -> Option<Saved> {
    KvStore::open(path).ok()?.get_json(KEY).ok()?
}

fn save(path: &Path, saved: &Saved) -> KvResult<()> {
    KvStore::open(path)?.set_json(KEY, saved)
}
