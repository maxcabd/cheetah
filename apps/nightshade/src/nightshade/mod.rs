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
use crate::router::Router;
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

/// Suggestions when there is nothing better (no history, backend offline).
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
    /// The map centers on the user (locate button) until a card, pin or
    /// search takes it back.
    locating: bool,
    /// Locate taps, so each one moves the map even if it hasn't changed.
    locate_taps: u32,
    /// Result cards shrunk to small bars (swipe a card's handle down).
    cards_minimized: bool,
    /// First and last result cards built so far (`carousel_model`).
    cards_built: std::cell::Cell<Option<(usize, usize)>>,
    /// Bookmarked place ids (the bookmark button), kept across launches.
    saved_places: std::collections::BTreeSet<String>,
    /// The user's own searches, newest first, kept across launches.
    recent_searches: Vec<String>,
    /// From the backend: trending searches and ones for the time of day.
    trending: Vec<String>,
    time_of_day: Vec<String>,
    /// The suggestions request in flight.
    suggestions_request: Option<u64>,
    /// Pages over the map (`views/pages/`): the restaurant page.
    router: Router,
    /// Pages opened so far: each gets a fresh native page (`page:<n>`), so
    /// a page iOS closed without telling us can't block the next one.
    page_opens: u32,
    /// Each place's photo's dominant color ("#rrggbb"), reported by the
    /// card's `photo` once its first image loads.
    photo_colors: std::collections::HashMap<String, String>,
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

    /// Searches `base_url`, whatever the build was made for (benchmarks).
    pub fn remote(base_url: &str) -> Self {
        Self::with_backend(false, Backend::remote(base_url))
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
            locating: false,
            locate_taps: 0,
            cards_minimized: false,
            cards_built: Default::default(),
            saved_places: Default::default(),
            recent_searches: Vec::new(),
            trending: Vec::new(),
            time_of_day: Vec::new(),
            suggestions_request: None,
            photo_colors: std::collections::HashMap::new(),
            router: Router::from_views(&view_files()),
            page_opens: 0,
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

        app.load_suggestions();

        if demo {
            app.search = Resource::Ready(SearchResponse {
                places: local_places(),
                followups: vec![
                    "Cafes within 12 min walk".into(),
                    "Best dessert spots near me".into(),
                ],
                plan: None,
                notice: None,
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
            } else if event.payload == "active" {
                // The time of day may have moved on.
                self.load_suggestions();
            }
            return;
        }

        // A pressed `link` (a card): open its page. A minimized card's
        // tap brings the cards back instead.
        if event.kind == "navigate" {
            if self.cards_minimized {
                self.minimize_cards(false);
            } else {
                self.open_page(|router| router.handle_event(&event));
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
            // The user dragged the page down: it is already closed.
            "dismiss" if key.starts_with("page:") => {
                self.router.back();
            }
            "dismiss" if key == "search-panel" => {
                if self.expanded {
                    self.expanded = false;
                    self.field_mount += 1;
                    self.haptic("selection");
                }
            }
            "submit" if key == "search" => self.send_draft(),
            // The user dragged the sheet to a detent (`views/sheet.view`).
            "select" if key == "search-panel" => match event.payload.as_str() {
                "expanded" => self.expanded = true,
                "rest" => {
                    if self.expanded {
                        self.expanded = false;
                        // Remounting the field dismisses the keyboard.
                        self.field_mount += 1;
                    }
                }
                _ => {}
            },
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
            // A card photo loaded: remember its dominant color.
            "load" if key.starts_with("photo:") => {
                let id = &key["photo:".len()..];
                let color = serde_json::from_str::<serde_json::Value>(&event.payload)
                    .ok()
                    .and_then(|data| data["color"].as_str().map(str::to_string));
                if let Some(color) = color {
                    self.photo_colors.insert(id.to_string(), color);
                }
            }
            // A card's handle: swipe down to minimize the cards, up to restore.
            "gesture" if key.starts_with("handle:") => {
                if let Some(gesture) = crate::gesture::Gesture::parse(&event) {
                    if gesture.kind == "swipe" {
                        match gesture.direction.as_str() {
                            "down" => self.minimize_cards(true),
                            "up" => self.minimize_cards(false),
                            _ => {}
                        }
                    }
                }
            }
            "page" if key == "places" => {
                if let Ok(index) = event.payload.parse::<usize>() {
                    if self
                        .response()
                        .is_some_and(|response| index < response.places.len())
                    {
                        self.selected = index;
                        self.focused = true;
                        self.locating = false;
                        self.haptic("selection");
                    }
                }
            }
            "click" => {
                if let Some(index) = key.strip_prefix("suggest:") {
                    if let Ok(index) = index.parse::<usize>() {
                        if let Some(suggestion) = self.suggestions().get(index).cloned() {
                            self.run_search(&suggestion);
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
                } else if let Some(id) = key.strip_prefix("card:share:") {
                    self.share_place(&id.to_string());
                } else if let Some(id) = key.strip_prefix("card:directions:") {
                    self.directions(&id.to_string());
                } else if let Some(id) = key.strip_prefix("card:bookmark:") {
                    self.toggle_saved(&id.to_string());
                } else if let Some(id) = self.open_place_id().filter(|_| key == "page:share") {
                    self.share_place(&id);
                } else if let Some(id) = self.open_place_id().filter(|_| key == "page:bookmark") {
                    self.toggle_saved(&id);
                } else if let Some(id) = self.open_place_id().filter(|_| key == "page:directions") {
                    self.directions(&id);
                } else if let Some(id) = key.strip_prefix("card:up:") {
                    // The card's up arrow: its page, growing out of the card.
                    let path = format!("/restaurant/{}", crate::router::encode_segment(id));
                    let source = format!("card-{id}");
                    self.open_page(|router| router.push(&path, "zoom", Some(source)));
                } else if key == "page:back" {
                    self.router.back();
                } else if key == "map:locate" {
                    self.locate();
                } else if key == "map:type" {
                    self.satellite = !self.satellite;
                    self.haptic("selection");
                } else if key == "show-on-map" {
                    self.expanded = false;
                    // Remounting the field dismisses the keyboard.
                    self.field_mount += 1;
                } else if let Some(id) = key.strip_prefix("place:") {
                    self.select_place(id);
                } else if key.starts_with("handle:") {
                    self.minimize_cards(!self.cards_minimized);
                } else if key.starts_with("place-") && self.cards_minimized {
                    // Tapping a small card brings the cards back.
                    self.minimize_cards(false);
                } else if key.starts_with("card:") {
                    // Card actions are visual for now.
                }
            }
            _ => {}
        }
    }

    fn take_native_calls(&mut self) -> Vec<NativeCall> {
        let mut calls = std::mem::take(&mut self.calls);
        // The host's back gesture is on while a page is open.
        calls.extend(self.router.take_sync());
        calls
    }

    fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    fn take_views(&mut self) -> Option<Vec<(String, String)>> {
        self.updates.as_mut()?.take_views()
    }

    fn views(&self) -> Vec<(&'static str, &'static str)> {
        view_files()
    }
}

impl NightshadeApp {
    /// Open a page from the map. A page still open here was closed by the
    /// user without its `dismiss` arriving (you can only tap a card when no
    /// page covers it): close it first, so the stack doesn't grow.
    pub(crate) fn open_page(&mut self, open: impl FnOnce(&mut Router) -> bool) {
        if self.router.current().page.is_some() {
            self.router.back();
        }
        if open(&mut self.router) {
            self.page_opens += 1;
            self.haptic("light");
            if let Some(id) = self.open_place_id() {
                self.report("open", &id);
            }
        }
    }
}

impl NightshadeApp {
    /// The place whose page is open, if any.
    fn open_place_id(&self) -> Option<String> {
        let location = self.router.current();
        (location.page.as_deref() == Some("RestaurantPage"))
            .then(|| location.params.get("id").cloned())
            .flatten()
    }

    fn place(&self, id: &str) -> Option<&Place> {
        self.visible_places().iter().find(|place| place.id == id)
    }

    /// The system share sheet with the place's name, address and a map link.
    fn share_place(&mut self, id: &str) {
        let Some(place) = self.place(id) else { return };
        let mut text = place.name.clone();
        if !place.address.is_empty() {
            text.push_str(&format!(", {}", place.address));
        }
        text.push_str(&format!("\n{}", maps_link(place, false)));
        self.calls.push(NativeCall::new(
            "system",
            "share",
            &serde_json::json!({"text": text}).to_string(),
        ));
        self.haptic("light");
        self.report("share", id);
    }

    /// Directions to the place: ask which maps app (an action sheet), then
    /// open it (`directions_chosen`).
    fn directions(&mut self, id: &str) {
        if self.place(id).is_none() {
            return;
        }
        self.calls.push(NativeCall::new(
            "dialog",
            "actionSheet",
            &serde_json::json!({
                "id": format!("directions:{id}"),
                "title": "Directions",
                "options": MAPS_APPS,
            })
            .to_string(),
        ));
        self.haptic("light");
    }

    /// The maps app picked for directions: 0 Apple Maps, 1 Google Maps,
    /// -1 cancelled.
    pub(crate) fn directions_chosen(&mut self, id: &str, choice: i64) {
        let Some(place) = self.place(id) else { return };
        let url = match choice {
            0 => maps_link(place, true),
            // Google's universal link opens the Google Maps app when it's
            // installed, else the website.
            1 => format!(
                "https://www.google.com/maps/dir/?api=1&destination={},{}",
                place.lat, place.lng
            ),
            _ => return,
        };
        self.calls.push(NativeCall::new(
            "system",
            "openUrl",
            &serde_json::json!({"url": url}).to_string(),
        ));
        self.report("directions", id);
    }

    /// Bookmark or un-bookmark a place.
    fn toggle_saved(&mut self, id: &str) {
        if !self.saved_places.remove(id) {
            self.saved_places.insert(id.to_string());
            self.report("save", id);
        }
        self.haptic("selection");
    }

    /// Tell the backend what the user did with a result (open, directions,
    /// save, share) for the search that showed it, so ranking can learn.
    /// Only for places in the current results; no reply is awaited.
    fn report(&mut self, action: &str, id: &str) {
        let Some(response) = self.response() else { return };
        let Some(position) = response.places.iter().position(|place| place.id == id) else {
            return;
        };
        // The plan's query_text includes follow-ups: what ranked this list.
        let query = response
            .plan
            .as_ref()
            .and_then(|plan| plan["query_text"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.query.clone());
        let Backend::Remote { client, .. } = &mut self.backend else { return };
        let request = client.post("/v1/search/feedback").json(&serde_json::json!({
            "query": query,
            "place_id": id,
            "action": action,
            "position": position,
        }));
        self.calls.push(request.into_call());
    }

    pub(crate) fn is_saved(&self, id: &str) -> bool {
        self.saved_places.contains(id)
    }
}

/// The maps apps directions can open in, in action sheet order.
const MAPS_APPS: [&str; 2] = ["Apple Maps", "Google Maps"];

/// An Apple Maps link to `place`: its pin, or directions to it.
fn maps_link(place: &Place, directions: bool) -> String {
    let name: String = place
        .name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect();
    let at = format!("{},{}", place.lat, place.lng);
    if directions {
        format!("https://maps.apple.com/?daddr={at}&q={name}")
    } else {
        format!("https://maps.apple.com/?ll={at}&q={name}")
    }
}

impl NightshadeApp {
    /// Ask the backend for trending and time-of-day suggestions.
    pub(crate) fn load_suggestions(&mut self) {
        let Backend::Remote { client, .. } = &mut self.backend else {
            return;
        };
        let request = client.get("/v1/search/suggestions");
        self.suggestions_request = Some(request.id);
        self.calls.push(request.into_call());
    }

    /// The suggestion chips: up to two of the user's recent searches, two
    /// trending, two for the time of day; topped up from the fixed list.
    pub(crate) fn suggestions(&self) -> Vec<String> {
        let mut picked: Vec<String> = Vec::new();
        let add = |text: &str, picked: &mut Vec<String>| {
            if picked.len() < 6 && !picked.iter().any(|p| p.eq_ignore_ascii_case(text)) {
                picked.push(text.to_string());
            }
        };
        for (source, take) in [
            (&self.recent_searches, 2),
            (&self.trending, 2),
            (&self.time_of_day, 2),
        ] {
            for text in source.iter().take(take) {
                add(text, &mut picked);
            }
        }
        // Fill up: more of each, then the fixed list.
        for text in self.time_of_day.iter().chain(&self.trending).chain(&self.recent_searches) {
            add(text, &mut picked);
        }
        for text in SUGGESTIONS {
            add(text, &mut picked);
        }
        picked
    }

    /// Remember a new search the user made (not follow-ups).
    pub(crate) fn remember_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        self.recent_searches.retain(|q| !q.eq_ignore_ascii_case(query));
        self.recent_searches.insert(0, query.to_string());
        self.recent_searches.truncate(RECENT_SEARCHES);
    }
}

/// How many of the user's searches are kept.
const RECENT_SEARCHES: usize = 10;

/// The app's `.view` files. Files under `views/pages/` are router pages.
fn view_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("views/theme.view", include_str!("views/theme.view")),
        ("views/widgets.view", include_str!("views/widgets.view")),
        ("views/card.view", include_str!("views/card.view")),
        ("views/sheet.view", include_str!("views/sheet.view")),
        ("views/map.view", include_str!("views/map.view")),
        (
            "views/pages/restaurant/[id].view",
            include_str!("views/pages/restaurant/[id].view"),
        ),
    ]
}
