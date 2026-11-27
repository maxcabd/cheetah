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
    /// Map and card choices; absent in state saved by older versions.
    #[serde(default)]
    satellite: bool,
    #[serde(default)]
    cards_minimized: bool,
    /// Every bookmark's id, as older versions kept them.
    #[serde(default)]
    saved_places: Vec<String>,
    /// The bookmarks' places, newest first; absent before the saved list.
    #[serde(default)]
    saved_details: Vec<Place>,
    #[serde(default)]
    recent_searches: Vec<String>,
    /// Settings; absent in state saved before them.
    #[serde(default)]
    maps_app: Option<usize>,
    #[serde(default)]
    miles: bool,
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
            results: self.response().cloned().map(|mut results| {
                results.places = results.places.iter().map(without_google_photos).collect();
                results
            }),
            selected: self.selected,
            focused: self.focused,
            center: self.center,
            satellite: self.satellite,
            cards_minimized: self.cards_minimized,
            saved_places: self
                .saved
                .iter()
                .map(|place| place.id.clone())
                .chain(self.saved_unseen.iter().cloned())
                .collect(),
            saved_details: self.saved.iter().map(without_google_photos).collect(),
            recent_searches: self.recent_searches.clone(),
            maps_app: self.maps_app,
            miles: self.miles,
        }
    }

    fn restore(&mut self, saved: Saved) {
        self.center = saved.center.or(self.center);
        self.satellite = saved.satellite;
        self.saved_unseen = saved
            .saved_places
            .into_iter()
            .filter(|id| !saved.saved_details.iter().any(|place| &place.id == id))
            .collect();
        self.saved = saved.saved_details;
        self.recent_searches = saved.recent_searches;
        self.maps_app = saved.maps_app.filter(|&app| app < MAPS_APPS.len());
        self.miles = saved.miles;
        match saved.results {
            Some(results) => {
                self.cards_minimized = saved.cards_minimized;
                self.selected = saved.selected.min(results.places.len().saturating_sub(1));
                self.focused = saved.focused;
                self.query = saved.query;
                let ids = results.places.iter().map(|place| place.id.clone()).collect();
                self.search = Resource::Ready(results);
                self.refresh_saved();
                // Google's photos weren't kept: the cards' current ones.
                self.request_photos(ids);
                self.build_ahead();
            }
            // A search still running when the app was closed runs again.
            None if !saved.query.is_empty() => self.run_search(&saved.query),
            None => {}
        }
        self.draft = saved.draft;
    }
}

/// A place as kept on the phone (saved, or in the last results): without
/// Google's photos, whose URLs expire and whose terms don't allow keeping
/// them (the ones with a credit). They're fetched again (`request_photos`)
/// at launch for the results, and when the saved list opens.
fn without_google_photos(place: &Place) -> Place {
    let mut kept = place.clone();
    let credited = |index: usize| place.photo_credits.get(index).is_some_and(|credit| !credit.name.is_empty());
    kept.photos = place
        .photos
        .iter()
        .enumerate()
        .filter(|&(index, _)| !credited(index))
        .map(|(_, photo)| photo.clone())
        .collect();
    kept.photo_credits = Vec::new();
    kept
}

// Opened per call: the file is small, and redb locks it while open, which a
// dev server reconnecting before the old session closes would trip over.
fn load(path: &Path) -> Option<Saved> {
    KvStore::open(path).ok()?.get_json(KEY).ok()?
}

fn save(path: &Path, saved: &Saved) -> KvResult<()> {
    KvStore::open(path)?.set_json(KEY, saved)
}
