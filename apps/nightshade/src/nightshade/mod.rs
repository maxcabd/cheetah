//! Nightshade: map, search sheet, and place cards.
//!
//! Device builds compile this module into the runtime library; the
//! apps/nightshade binary compiles it for the hot-reloading dev server.
//! Split by area so an edit recompiles only that file's code.

use crate::app::{App, Event, HostInfo, ViewContext};
use crate::element::{column, glass, hscroll, input, map, native, row, sheet, text, Element};
use crate::http::{HttpClient, HttpResponse};
use crate::native_api::{NativeCall, NativeEvent};
use crate::resource::Resource;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;

mod card;
mod data;
mod map;
mod search;
mod sheet;
#[cfg(test)]
mod tests;
mod widgets;

use self::data::*;

const BG: &str = "#0b0b0f";
const SHEET_BG: &str = "#171717";
const CARD_BG: &str = "#1f1f1f";
const CHIP_BG: &str = "#242424";
const TEXT: &str = "#f5f5f5";
const MUTED: &str = "#a3a3a3";
const GREEN: &str = "#4ade80";
const RED: &str = "#f87171";
const SEARCH_HEIGHT: f32 = 72.0;
const RESULT_SHEET_HEIGHT: f32 = 205.0;
const CAROUSEL_BOTTOM: f32 = 225.0;
const CARD_HEIGHT: f32 = 320.0;
const CARD_HEADER: f32 = 180.0;
const SEARCH_BOTTOM_GAP: f32 = 112.0;

const INITIAL_LAT: f32 = 37.7749;
const INITIAL_LNG: f32 = -122.4194;

const SUGGESTIONS: [&str; 7] = [
    "Cafes within 12 min walk",
    "Best dessert spots near me",
    "Halal spots in Ottawa",
    "Best brunch spots",
    "Casual catch up with friends",
    "Nearby spots that open late night",
    "Amazing pizza places",
];

pub struct NightshadeApp {
    host: HostInfo,
    draft: String,
    query: String,
    search: Resource<SearchResponse>,
    backend: Backend,
    sample_places: Vec<Place>,
    selected: usize,
    focused: bool,
    expanded: bool,
    center: Option<(f32, f32)>,
    calls: Vec<NativeCall>,
    suggestion_cache: RefCell<Option<Element>>,
    card_cache: RefCell<HashMap<(String, u32), Element>>,
}

impl NightshadeApp {
    /// Searches `NIGHTSHADE_API_URL` when it was set at build time; the demo
    /// always uses local sample data.
    pub fn new(demo: bool) -> Self {
        let backend = match API_URL {
            Some(url) if !demo && !url.is_empty() => Backend::remote(url),
            _ => Backend::Local(Box::new(LocalPlaceSource)),
        };
        Self::with_backend(demo, backend)
    }

    fn with_backend(demo: bool, backend: Backend) -> Self {
        let mut app = Self {
            host: HostInfo::default(),
            draft: String::new(),
            query: String::new(),
            search: Resource::Idle,
            backend,
            sample_places: local_places(),
            selected: 0,
            focused: false,
            expanded: false,
            center: None,
            calls: vec![NativeCall::new("location", "request", "{}")],
            suggestion_cache: RefCell::new(None),
            card_cache: RefCell::new(HashMap::new()),
        };

        if demo {
            app.search = Resource::Ready(SearchResponse {
                places: local_places(),
                followups: vec![
                    "Cafes within 12 min walk".into(),
                    "Best dessert spots near me".into(),
                ],
            });
            app.query = "Coffee".into();
        }

        app
    }

    fn response(&self) -> Option<&SearchResponse> {
        self.search.ready()
    }

    fn has_results(&self) -> bool {
        self.search.is_ready()
    }
}

impl Default for NightshadeApp {
    fn default() -> Self {
        Self::new(false)
    }
}

impl App for NightshadeApp {
    fn view(&self, context: &ViewContext) -> Element {
        let (latitude, longitude, latitude_delta, longitude_delta) = self.region();
        let mut root = column().width(-1.0).height(-1.0).bg(BG).child(
            map()
                .key("map")
                .width(-1.0)
                .height(-1.0)
                .bg("#101826")
                .test_id("nightshade-map")
                .map_region(latitude, longitude, latitude_delta, longitude_delta)
                .map_markers(self.markers())
                .map_dark(true)
                .map_shows_user(true),
        );

        if self.has_results() {
            root = root.child(self.carousel(context, self.visible_places()));
        }

        root.child(self.sheet(context))
    }

    fn update(&mut self, event: Event) {
        if event.kind == "host" {
            if let Ok(host) = serde_json::from_str::<HostInfo>(&event.payload) {
                self.host = host;
            }
            return;
        }

        if event.kind == "native" {
            self.handle_native_event(&event.payload);
            return;
        }

        let Some(key) = event.key.as_deref() else {
            return;
        };

        match event.kind.as_str() {
            "change" if key == "search" => {
                self.draft = event.payload;
            }
            "focus" if key == "search" => {
                self.expanded = true;
            }
            "blur" if key == "search" => {
                self.expanded = false;
            }
            "submit" if key == "search" => {
                let query = self.draft.clone();
                self.run_search(&query);
            }
            "select" if key == "open-menu" => match event.payload.as_str() {
                "Coffee" => self.run_search("coffee"),
                "Sushi" => self.run_search("sushi"),
                "Pizza" => self.run_search("pizza"),
                "Open now" => self.run_search("open"),
                _ => {}
            },
            "marker" => {
                self.select_place(&event.payload);
            }
            "page" if key == "places" => {
                if let Ok(index) = event.payload.parse::<usize>() {
                    if self
                        .response()
                        .is_some_and(|response| index < response.places.len())
                    {
                        self.selected = index;
                        self.focused = true;
                        self.haptic("selection");
                    }
                }
            }
            "click" => {
                if let Some(index) = key.strip_prefix("suggest:") {
                    if let Ok(index) = index.parse::<usize>() {
                        if let Some(suggestion) = SUGGESTIONS.get(index) {
                            self.run_search(suggestion);
                        }
                    }
                } else if let Some(index) = key.strip_prefix("followup:") {
                    if let Ok(index) = index.parse::<usize>() {
                        if let Some(followup) = self
                            .response()
                            .and_then(|response| response.followups.get(index))
                            .cloned()
                        {
                            self.run_search(&followup);
                        }
                    }
                } else if key == "sheet:toggle" {
                    self.expanded = !self.expanded;
                    self.haptic("selection");
                } else if key == "search:send" {
                    let query = self.draft.clone();
                    self.run_search(&query);
                } else if key == "search:clear" {
                    self.clear_search();
                } else if key == "show-on-map" {
                    self.expanded = false;
                } else if let Some(id) = key.strip_prefix("place:") {
                    self.select_place(id);
                } else if key.starts_with("card:") {
                    // Card actions are visual for now.
                }
            }
            _ => {}
        }
    }

    fn take_native_calls(&mut self) -> Vec<NativeCall> {
        std::mem::take(&mut self.calls)
    }

    fn hot_reloaded(&mut self) {
        // Memoized elements were built by the old view code.
        self.suggestion_cache.borrow_mut().take();
        self.card_cache.borrow_mut().clear();
    }
}
