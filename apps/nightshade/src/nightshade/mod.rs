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
    /// The user said no to location: the locate button offers Settings.
    location_denied: bool,
    /// The side drawer (the map's menu button): the map screen slides
    /// aside to show it.
    drawer_open: bool,
    /// The map screen keeps its rounded corners until it has slid back.
    drawer_rounded: bool,
    /// A finger is dragging the map screen (the host moves it; see
    /// `drag` in views/map.view) until it snaps open or shut.
    drawer_dragging: bool,
    /// Result cards shrunk to small bars (swipe a card's handle down).
    cards_minimized: bool,
    /// First and last result cards built so far (`carousel_model`).
    cards_built: std::cell::Cell<Option<(usize, usize)>>,
    /// A timer to build the next card is pending; at most one runs.
    cards_timer: bool,
    /// The cards moved since that timer started: let them settle first.
    cards_moved: bool,
    /// Bookmarked places (the bookmark button), newest first, kept across
    /// launches with what's needed to list and open them (`/saved`).
    saved: Vec<Place>,
    /// Bookmarks saved by older versions, which kept only the id: listed
    /// once the place turns up in results again (`refresh_saved`).
    saved_unseen: std::collections::BTreeSet<String>,
    /// The saved list's filter: a cuisine, or `None` for all.
    saved_filter: Option<String>,
    /// Settings: the maps app Directions opens straight away (an index into
    /// `MAPS_APPS`), or `None` to ask each time.
    maps_app: Option<usize>,
    /// Settings: distances in miles and feet, not kilometres and metres.
    miles: bool,
    /// The user's own searches, newest first, kept across launches.
    recent_searches: Vec<String>,
    /// How often, and when, the user makes each search (`state/habits.rs`):
    /// the suggestion chips lead with them. Kept across launches.
    habits: Vec<state::habits::Habit>,
    /// The phone's offset from UTC, learned from the backend's times; the
    /// time of day for habits.
    utc_offset_min: Option<i32>,
    /// Tests: the time, in seconds since 1970 (UTC), instead of the clock.
    now_override: Option<i64>,
    /// From the backend: trending searches and ones for the time of day.
    trending: Vec<String>,
    time_of_day: Vec<String>,
    /// The suggestions request in flight.
    suggestions_request: Option<u64>,
    /// Requests for places' current photos (`request_photos`): the saved
    /// list's as it opens, the last results' at launch.
    photos_requests: std::collections::HashSet<u64>,
    /// Pages over the map (`views/pages/`): the restaurant page.
    router: Router,
    /// Pages opened so far: each gets a fresh native page (`page:<n>`), so
    /// a page iOS closed without telling us can't block the next one.
    page_opens: u32,
    /// Each place's photo's dominant color ("#rrggbb"), reported by the
    /// card's `photo` once its first image loads.
    photo_colors: std::collections::HashMap<String, String>,
    /// The photo each pager shows (by its key, "photo:<id>"), so its credit
    /// shows with it; missing is the first.
    photo_pages: std::collections::HashMap<String, usize>,
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
            location_denied: false,
            drawer_open: false,
            drawer_rounded: false,
            drawer_dragging: false,
            cards_minimized: false,
            cards_built: Default::default(),
            cards_timer: false,
            cards_moved: false,
            saved: Vec::new(),
            saved_unseen: Default::default(),
            saved_filter: None,
            maps_app: None,
            miles: false,
            recent_searches: Vec::new(),
            habits: Vec::new(),
            utc_offset_min: None,
            now_override: None,
            trending: Vec::new(),
            time_of_day: Vec::new(),
            suggestions_request: None,
            photos_requests: Default::default(),
            photo_colors: std::collections::HashMap::new(),
            photo_pages: std::collections::HashMap::new(),
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

    /// Places to show: a search that found nothing has none, and shows
    /// its own state in the sheet instead of empty cards.
    fn has_results(&self) -> bool {
        self.response()
            .is_some_and(|response| !response.places.is_empty())
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
                // Back from Settings, location may be allowed now.
                if self.location_denied {
                    self.calls.push(NativeCall::new("location", "request", "{}"));
                }
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
            match event.payload.as_str() {
                "searching" => self.searching_tick(),
                "cards" => self.cards_tick(),
                "drawer" => self.drawer_settled(),
                _ => {}
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
            "select" if key == "saved:filter" => self.filter_saved(&event.payload),
            "select" if key == "settings:maps" => self.choose_maps_app(&event.payload),
            "select" if key == "settings:units" => self.choose_units(&event.payload),
            // A maps app picked from a directions menu: a card's, or the
            // open place page's.
            "select" if key.starts_with("card:directions:") || key == "page:directions" => {
                let id = match key.strip_prefix("card:directions:") {
                    Some(id) => Some(id.to_string()),
                    None => self.open_place_id(),
                };
                let choice = MAPS_APPS.iter().position(|app| *app == event.payload);
                if let (Some(id), Some(choice)) = (id, choice) {
                    self.directions_chosen(&id, choice as i64);
                }
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
            // A finger took the map screen, and where it let it rest.
            "drag" if key == "screen" => self.drawer_drag_began(),
            "snap" if key == "screen" => {
                let index = serde_json::from_str::<serde_json::Value>(&event.payload)
                    .ok()
                    .and_then(|snap| snap["index"].as_u64())
                    .unwrap_or(0);
                self.drawer_snapped(index == 1);
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
            // A card's or page's photos paged: its credit follows.
            "page" if key.starts_with("photo:") || key.starts_with("page-photo:") => {
                if let Ok(index) = event.payload.parse::<usize>() {
                    self.photo_pages.insert(key.to_string(), index);
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
                        self.build_ahead();
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
                } else if key == "search:retry" {
                    self.retry_search();
                } else if key == "drawer:open" {
                    self.open_drawer();
                } else if key == "drawer:close" {
                    self.close_drawer();
                } else if key == "drawer:saved" {
                    self.close_drawer();
                    self.open_saved();
                } else if key == "drawer:settings" {
                    self.close_drawer();
                    self.open_page(|router| router.push("/settings", "push", None));
                } else if key == "settings:location" {
                    self.open_settings();
                } else if key == "settings:clear-recents" || key == "settings:clear-saved" {
                    self.confirm_clear(&key.to_string());
                } else if key.starts_with("card:directions:") || key == "page:directions" {
                    // A maps app chosen in Settings: Directions goes straight
                    // there (without one, the button is a menu instead).
                    let id = match key.strip_prefix("card:directions:") {
                        Some(id) => Some(id.to_string()),
                        None => self.open_place_id(),
                    };
                    if let (Some(id), Some(app)) = (id, self.maps_app) {
                        self.haptic("light");
                        self.directions_chosen(&id, app as i64);
                    }
                } else if let Some(pager) = key.strip_prefix("credit:") {
                    self.open_photo_credit(&pager.to_string());
                } else if let Some(id) = key.strip_prefix("saved:open:") {
                    self.open_saved_place(&id.to_string());
                } else if let Some(id) = key.strip_prefix("card:share:") {
                    self.share_place(&id.to_string());
                } else if let Some(id) = key.strip_prefix("card:bookmark:") {
                    self.toggle_saved(&id.to_string());
                } else if let Some(id) = self.open_place_id().filter(|_| key == "page:share") {
                    self.share_place(&id);
                } else if let Some(id) = self.open_place_id().filter(|_| key == "page:bookmark") {
                    self.toggle_saved(&id);
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

    /// A place in the results, else a saved one.
    fn place(&self, id: &str) -> Option<&Place> {
        self.visible_places()
            .iter()
            .chain(&self.saved)
            .find(|place| place.id == id)
    }

    /// Whether `id` is in the current results, not only saved.
    pub(crate) fn in_results(&self, id: &str) -> bool {
        self.response()
            .is_some_and(|response| response.places.iter().any(|place| place.id == id))
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

    /// The maps app picked from a directions menu: 0 Apple Maps, 1 Google
    /// Maps (`MAPS_APPS`).
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
        if self.is_saved(id) {
            self.saved.retain(|place| place.id != id);
            self.saved_unseen.remove(id);
        } else if let Some(place) = self.place(id).cloned() {
            // The system's confirmation: "Saved", and which place.
            self.calls.push(NativeCall::new(
                "system",
                "toast",
                &serde_json::json!({"title": "Saved", "text": place.name, "icon": "bookmark.fill"})
                    .to_string(),
            ));
            self.saved.insert(0, place);
            self.report("save", id);
        } else {
            return;
        }
        self.haptic("selection");
    }

    /// New results: saved places among them get their latest details, and
    /// old id-only bookmarks get theirs for the first time.
    pub(crate) fn refresh_saved(&mut self) {
        let Some(response) = self.search.ready() else { return };
        for place in &response.places {
            if let Some(saved) = self.saved.iter_mut().find(|saved| saved.id == place.id) {
                *saved = place.clone();
            } else if self.saved_unseen.remove(&place.id) {
                self.saved.push(place.clone());
            }
        }
    }

    pub(crate) fn saved_places(&self) -> &[Place] {
        &self.saved
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
        self.saved.iter().any(|place| place.id == id) || self.saved_unseen.contains(id)
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

    /// The suggestion chips: the user's habits for this time of day first
    /// (up to two), then one for the time of day, their latest search and
    /// one trending; topped up with more of each, then the fixed list.
    pub(crate) fn suggestions(&self) -> Vec<String> {
        let mut picked: Vec<String> = Vec::new();
        let add = |text: &str, picked: &mut Vec<String>| {
            if picked.len() < 6 && !picked.iter().any(|p| p.eq_ignore_ascii_case(text)) {
                picked.push(text.to_string());
            }
        };
        for text in self.habits_now().into_iter().take(2) {
            add(text, &mut picked);
        }
        for source in [&self.time_of_day, &self.recent_searches, &self.trending] {
            if let Some(text) = source.iter().find(|text| !picked.iter().any(|p| p.eq_ignore_ascii_case(text))) {
                add(text, &mut picked);
            }
        }
        // Fill up: more of each, then the fixed list.
        for text in self.time_of_day.iter().chain(&self.recent_searches).chain(&self.trending) {
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
        self.record_habit(query);
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
        ("views/pages/saved.view", include_str!("views/pages/saved.view")),
        ("views/pages/settings.view", include_str!("views/pages/settings.view")),
        ("views/drawer.view", include_str!("views/drawer.view")),
    ]
}
