//! Nightshade: map, search sheet, and place cards.
//!
//! Device builds compile this module into the runtime library; the
//! apps/nightshade binary compiles it for the hot-reloading dev server.
//! Split by area so an edit recompiles only that file's code.

use crate::app::{Action, App, Event, HostInfo, ViewContext};
use crate::element::Element;
use crate::http::{HttpClient, HttpResponse};
use crate::native_api::{NativeCall, NativeEvent};
use crate::resource::Resource;
use crate::updates::Updates;
use serde::Deserialize;

mod api;
mod components;
mod screens;
mod state;
#[cfg(test)]
mod tests;
mod theme;

use self::api::*;
use self::theme::*;

/// Over-the-air views (`cheetah publish`): set NIGHTSHADE_VIEWS_URL at build
/// time to the published bundle's URL.
const VIEWS_URL: Option<&str> = option_env!("NIGHTSHADE_VIEWS_URL");
/// This build's runtime: `version+build` from cheetah.toml (a test checks).
const RUNTIME_VERSION: &str = "1.0.0+1";

const SUGGESTIONS: [&str; 6] = [
    "Cafes within 12 min walk",
    "Best dessert spots near me",
    "Halal spots in Ottawa",
    "Best brunch spots",
    "Casual catch up with friends",
    "Nearby spots that open late night",
];

/// Cycled while a search runs, like the React Native app.
const SEARCHING_EMOJIS: [&str; 8] = ["🍕", "🍔", "🍣", "🍜", "🌮", "🍝", "🥗", "🍱"];
const SEARCHING_FRAME_MS: u64 = 500;

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
    /// Bumped on send to remount the search field, which dismisses the
    /// keyboard: no host API can blur a field yet.
    field_mount: u32,
    center: Option<(f32, f32)>,
    /// Satellite imagery instead of the street map (top-right map button).
    satellite: bool,
    /// Where state is saved for the next launch (`with_state_file`).
    state_file: Option<std::path::PathBuf>,
    calls: Vec<NativeCall>,
    actions: Vec<Action>,
    /// Index into SEARCHING_EMOJIS while a search runs.
    searching_frame: usize,
    /// A searching timer is pending; at most one runs at a time.
    searching_timer: bool,
    updates: Option<Updates>,
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
            field_mount: 0,
            center: None,
            satellite: false,
            state_file: None,
            calls: vec![NativeCall::new("location", "request", "{}")],
            actions: Vec::new(),
            searching_frame: 0,
            searching_timer: false,
            updates: None,
        };

        if let Some(url) = VIEWS_URL.filter(|url| !url.is_empty() && !demo) {
            let mut updates = Updates::new(url, RUNTIME_VERSION);
            app.calls.extend(updates.start());
            app.updates = Some(updates);
        }

        if demo {
            app.search = Resource::Ready(SearchResponse {
                places: local_places(),
                followups: vec![
                    "Cafes within 12 min walk".into(),
                    "Best dessert spots near me".into(),
                ],
                plan: None,
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
        self.map_screen(context)
    }

    fn update(&mut self, event: Event) {
        if event.kind == "host" {
            if let Ok(host) = serde_json::from_str::<HostInfo>(&event.payload) {
                self.host = host;
            }
            return;
        }

        if let Some(updates) = &mut self.updates {
            if updates.on_event(&event) {
                return;
            }
        }

        if event.kind == "native" {
            self.handle_native_event(&event.payload);
            return;
        }

        // iOS may end a backgrounded app; save so the next launch resumes.
        if event.kind == "lifecycle" {
            if event.payload == "background" {
                self.save_state();
            }
            return;
        }

        if event.kind == "timer" {
            if event.payload == "searching" {
                self.searching_tick();
            }
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
            // Dragged down: an expanded sheet goes back to its resting
            // height, taking the keyboard with it.
            "dismiss" if key == "search-panel" => {
                if self.expanded {
                    self.expanded = false;
                    self.field_mount += 1;
                    self.haptic("selection");
                }
            }
            "submit" if key == "search" => self.send_draft(),
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
                    self.send_draft();
                } else if key == "search:clear" {
                    self.clear_search();
                } else if key == "map:type" {
                    self.satellite = !self.satellite;
                    self.haptic("selection");
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

    fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    fn take_views(&mut self) -> Option<Vec<(String, String)>> {
        self.updates.as_mut()?.take_views()
    }

    fn views(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("views/theme.view", include_str!("views/theme.view")),
            ("views/widgets.view", include_str!("views/widgets.view")),
            ("views/card.view", include_str!("views/card.view")),
            ("views/sheet.view", include_str!("views/sheet.view")),
            ("views/map.view", include_str!("views/map.view")),
        ]
    }
}
