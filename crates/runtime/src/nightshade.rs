use crate::app::{App, Event, HostInfo, ViewContext};
use crate::element::{column, glass, hscroll, input, map, native, row, sheet, text, Element};
use crate::http::{HttpClient, HttpResponse};
use crate::native_api::{NativeCall, NativeEvent};
use crate::resource::Resource;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;

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

/// Backend base URL baked in at build time, e.g.
/// `NIGHTSHADE_API_URL=http://10.0.2.2:8000` for the Android emulator.
/// Unset, Nightshade searches its local sample places.
const API_URL: Option<&str> = option_env!("NIGHTSHADE_API_URL");
const SEARCH_TIMEOUT_MS: u32 = 60_000;

#[derive(Clone, Debug, PartialEq)]
struct Place {
    id: String,
    name: String,
    address: String,
    lat: f32,
    lng: f32,
    distance_m: f32,
    price_tier: u8,
    /// `None` when the backend does not know the hours.
    open: Option<bool>,
    closes: String,
    opens: String,
    /// Empty when there is no rating to show.
    rating: String,
    reviews: String,
    cuisine: String,
    icon: String,
    icon_color: String,
}

#[allow(clippy::too_many_arguments)]
fn sample(
    id: &str,
    name: &str,
    address: &str,
    (lat, lng): (f32, f32),
    distance_m: f32,
    price_tier: u8,
    open: bool,
    (opens, closes): (&str, &str),
    cuisine: &str,
    (icon, icon_color): (&str, &str),
) -> Place {
    Place {
        id: id.into(),
        name: name.into(),
        address: address.into(),
        lat,
        lng,
        distance_m,
        price_tier,
        open: Some(open),
        closes: closes.into(),
        opens: opens.into(),
        rating: "4.3".into(),
        reviews: "50+".into(),
        cuisine: cuisine.into(),
        icon: icon.into(),
        icon_color: icon_color.into(),
    }
}

fn local_places() -> Vec<Place> {
    vec![
        sample(
            "java",
            "Java Haven",
            "Cedarwood Drive",
            (37.7793, -122.4192),
            280.0,
            2,
            true,
            ("7:00 AM", "6:00 PM"),
            "Coffee",
            ("coffee", "#7B3F00"),
        ),
        sample(
            "pizza",
            "Pizza Paradise",
            "Market Street",
            (37.7849, -122.4094),
            1_200.0,
            2,
            true,
            ("11:00 AM", "11:00 PM"),
            "Pizza",
            ("pizza-slice", "#FFA500"),
        ),
        sample(
            "sushi",
            "Sushi Zen",
            "Hayes Valley",
            (37.7694, -122.4862),
            700.0,
            3,
            true,
            ("11:30 AM", "9:30 PM"),
            "Sushi",
            ("fish", "#008080"),
        ),
        sample(
            "burger",
            "Burger Joint",
            "Mission Street",
            (37.7599, -122.4148),
            520.0,
            1,
            false,
            ("10:30 AM", "10:00 PM"),
            "Fast Food",
            ("hamburger", "#FF4500"),
        ),
    ]
}

#[derive(Clone, Debug, PartialEq)]
struct SearchResponse {
    places: Vec<Place>,
    followups: Vec<String>,
}

trait PlaceSource {
    fn search(&self, query: &str) -> Result<SearchResponse, String>;
}

struct LocalPlaceSource;

impl PlaceSource for LocalPlaceSource {
    fn search(&self, query: &str) -> Result<SearchResponse, String> {
        let lowered = query.to_lowercase();
        if lowered.contains("fail") {
            return Err("Place search is unavailable".into());
        }

        let places = local_places();
        let places = if lowered.contains("sushi") {
            vec![places[2].clone()]
        } else if lowered.contains("pizza") {
            vec![places[1].clone()]
        } else if lowered.contains("coffee")
            || lowered.contains("cafe")
            || lowered.contains("brunch")
        {
            vec![places[0].clone()]
        } else if lowered.contains("burger") || lowered.contains("fast") {
            vec![places[3].clone()]
        } else if lowered.contains("open") || lowered.contains("late") {
            places
                .into_iter()
                .filter(|place| place.open == Some(true))
                .collect()
        } else {
            places
        };

        Ok(SearchResponse {
            places,
            followups: vec![
                "Cafes within 12 min walk".into(),
                "Best dessert spots near me".into(),
                "Nearby spots that open late night".into(),
            ],
        })
    }
}

/// Where searches go. Remote searches are asynchronous: the request is
/// queued as an `http.request` native call and the result arrives as an
/// `http.response` event.
enum Backend {
    Local(Box<dyn PlaceSource>),
    Remote {
        client: HttpClient,
        pending: Option<u64>,
    },
}

impl Backend {
    fn remote(base_url: &str) -> Self {
        let mut client = HttpClient::new(base_url);
        client.set_timeout_ms(SEARCH_TIMEOUT_MS);
        Backend::Remote {
            client,
            pending: None,
        }
    }
}

/// `POST /v1/search` response (nightshadex backend). Only the fields the
/// map and cards use; everything is lenient because most are nullable.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiSearchResponse {
    cards: Vec<ApiCard>,
    followups: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiCard {
    place_id: String,
    name: String,
    address: Option<String>,
    lat: f32,
    lng: f32,
    distance_m: Option<f32>,
    price_tier: Option<u8>,
    open_now: Option<bool>,
    closes_at_local: Option<String>,
}

impl ApiCard {
    fn into_place(self) -> Place {
        Place {
            id: self.place_id,
            name: self.name,
            address: self.address.unwrap_or_default(),
            lat: self.lat,
            lng: self.lng,
            distance_m: self.distance_m.unwrap_or(0.0),
            price_tier: self.price_tier.unwrap_or(0),
            open: self.open_now,
            closes: self
                .closes_at_local
                .as_deref()
                .and_then(clock_time)
                .unwrap_or_default(),
            opens: String::new(),
            rating: String::new(),
            reviews: String::new(),
            cuisine: String::new(),
            icon: "utensils".into(),
            icon_color: "#ffffff".into(),
        }
    }
}

/// `"2026-11-15T22:30:00"` or `"22:30"` -> `"10:30 PM"`.
fn clock_time(value: &str) -> Option<String> {
    let time = value.split('T').nth(1).unwrap_or(value);
    let mut parts = time.split(':');
    let hour: u32 = parts.next()?.trim().parse().ok()?;
    let minute: u32 = parts.next()?.get(..2)?.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    Some(format!("{hour}:{minute:02} {suffix}"))
}

fn parse_search(response: &HttpResponse) -> Result<SearchResponse, String> {
    if !response.ok {
        return Err(match response.error.as_deref() {
            Some("timeout") => "Search timed out".into(),
            _ => "Can't reach Nightshade".into(),
        });
    }
    if !response.is_success() {
        return Err(format!("Search failed ({})", response.status));
    }
    let body: ApiSearchResponse = response
        .json()
        .map_err(|_| "Unexpected search response".to_string())?;
    Ok(SearchResponse {
        places: body.cards.into_iter().map(ApiCard::into_place).collect(),
        followups: body.followups,
    })
}

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

    fn sheet_height(&self, context: &ViewContext) -> f32 {
        if self.expanded {
            let fraction = if self.host.keyboard_inset > 0.0 {
                0.92
            } else {
                0.85
            };
            (context.viewport.height * fraction).max(520.0)
        } else if self.has_results() {
            RESULT_SHEET_HEIGHT
        } else {
            (context.viewport.height * 0.52).max(390.0)
        }
    }

    fn visible_places(&self) -> &[Place] {
        self.response()
            .map(|response| response.places.as_slice())
            .unwrap_or(&self.sample_places)
    }

    fn region(&self) -> (f32, f32, f32, f32) {
        if !self.has_results() {
            let (latitude, longitude) = self.center.unwrap_or((INITIAL_LAT, INITIAL_LNG));
            return (latitude, longitude, 0.025, 0.025);
        }

        let places = self.visible_places();
        let Some(first) = places.first() else {
            return (INITIAL_LAT, INITIAL_LNG, 0.025, 0.025);
        };

        if self.focused || places.len() == 1 {
            let place = places.get(self.selected).unwrap_or(first);
            let delta = 0.018;
            return (place.lat - delta * 0.40, place.lng, delta, delta);
        }

        let mut min_lat = first.lat;
        let mut max_lat = first.lat;
        let mut min_lng = first.lng;
        let mut max_lng = first.lng;
        for place in places {
            min_lat = min_lat.min(place.lat);
            max_lat = max_lat.max(place.lat);
            min_lng = min_lng.min(place.lng);
            max_lng = max_lng.max(place.lng);
        }

        let latitude = (min_lat + max_lat) / 2.0;
        let longitude = (min_lng + max_lng) / 2.0;
        let latitude_delta = ((max_lat - min_lat) * 1.4).max(0.012);
        let longitude_delta = ((max_lng - min_lng) * 1.4).max(0.012);
        // Keep result pins above the card carousel and search bar.
        let latitude = latitude - latitude_delta * 0.32;
        (latitude, longitude, latitude_delta, longitude_delta)
    }

    fn markers(&self) -> String {
        let results = self.has_results();
        let items = self
            .visible_places()
            .iter()
            .enumerate()
            .map(|(index, place)| {
                serde_json::json!({
                    "id": place.id,
                    "name": place.name,
                    "lat": place.lat,
                    "lng": place.lng,
                    "kind": if results { "result" } else { "static" },
                    "icon": if results { "utensils" } else { place.icon.as_str() },
                    "iconColor": if results { "#ffffff" } else { place.icon_color.as_str() },
                    "selected": results && index == self.selected,
                })
            })
            .collect::<Vec<_>>();
        serde_json::Value::Array(items).to_string()
    }

    fn icon(name: &str, size: f32, color: &str) -> Element {
        native("icon")
            .native_props(format!(r#"{{"name":"{name}","color":"{color}"}}"#))
            .width(size)
            .height(size)
            .size(size)
            .color(color)
    }

    fn glass_icon(icon_name: &str, key: &str) -> Element {
        glass()
            .key(key)
            .pressable(true)
            .width(38.0)
            .height(38.0)
            .radius(19.0)
            .glass_style("regular")
            .glass_tint("#00000047")
            .glass_interactive(true)
            .align_items("center")
            .justify("center")
            .child(Self::icon(icon_name, 16.0, "#ffffff"))
    }

    fn action_button(icon_name: &str, key: &str, background: &str, size: f32) -> Element {
        row()
            .key(key)
            .pressable(true)
            .width(size)
            .height(size)
            .radius(size / 2.0)
            .bg(background)
            .align_items("center")
            .justify("center")
            .child(Self::icon(icon_name, 16.0, "#ffffff"))
    }

    fn chip(label: &str, key: &str) -> Element {
        text(label)
            .key(key)
            .pressable(true)
            .size(14.0)
            .font("satoshi-bold")
            .color("#ffffff")
            .bg(CHIP_BG)
            .border(1.0, "#ffffff14")
            .pad_h(14.0)
            .width(label.chars().count() as f32 * 8.0 + 30.0)
            .height(42.0)
            .lines(1)
            .align("center")
            .radius(18.0)
    }

    fn card(&self, width: f32, place: &Place, _selected: bool) -> Element {
        let cache_key = (place.id.clone(), width.to_bits());
        if let Some(cached) = self.card_cache.borrow().get(&cache_key) {
            return cached.clone();
        }
        let drive_minutes = ((place.distance_m / 1000.0) / 30.0 * 60.0).round().max(1.0) as u32;
        let distance = if place.distance_m < 1000.0 {
            format!("{}m", place.distance_m.round() as u32)
        } else {
            format!("{:.1}km", place.distance_m / 1000.0)
        };
        let price = "$".repeat(place.price_tier.min(4) as usize);
        let status = if place.open.is_none() {
            row().width(width - 40.0).height(18.0).child(
                text("Hours unavailable")
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffffb3")
                    .width(160.0)
                    .height(18.0),
            )
        } else if place.open == Some(true) {
            row()
                .width(width - 40.0)
                .height(18.0)
                .gap(4.0)
                .align_items("center")
                .child(
                    text("Open")
                        .size(14.0)
                        .font("satoshi-bold")
                        .color(GREEN)
                        .width(38.0)
                        .height(18.0),
                )
                .child(
                    text("\u{00B7}")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(if place.closes.is_empty() {
                        String::new()
                    } else {
                        format!("Closes at {}", place.closes)
                    })
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffff")
                    .width(130.0)
                    .height(18.0),
                )
        } else {
            row()
                .width(width - 40.0)
                .height(18.0)
                .gap(4.0)
                .align_items("center")
                .child(
                    text("Closed")
                        .size(14.0)
                        .font("satoshi-bold")
                        .color(RED)
                        .width(48.0)
                        .height(18.0),
                )
                .child(
                    text("\u{00B7}")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(if place.opens.is_empty() {
                        String::new()
                    } else {
                        format!("Opens at {}", place.opens)
                    })
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffffb3")
                    .width(130.0)
                    .height(18.0),
                )
        };

        let top_controls = row()
            .absolute()
            .top(16.0)
            .left(16.0)
            .width(width - 32.0)
            .height(38.0)
            .justify("space-between")
            .align_items("center")
            .child(Self::glass_icon("chevron-up", "card:up"))
            .child(
                row()
                    .height(38.0)
                    .gap(8.0)
                    .align_items("center")
                    .child(Self::glass_icon("bookmark", "card:bookmark"))
                    .child(Self::glass_icon("share", "card:share"))
                    .child(Self::glass_icon("plus", "card:plus")),
            );

        let photo = native("photo")
            .absolute()
            .top(0.0)
            .left(0.0)
            .width(width)
            .height(CARD_HEADER)
            .native_props(r##"{"src":"asset://second-cup-card-full.jpeg","pages":5}"##);

        let card = column()
            .key(format!("place-{}", place.id))
            .pressable(true)
            .animate(true)
            .width(width)
            .height(CARD_HEIGHT)
            .bg(CARD_BG)
            .radius(28.0)
            .clip(true)
            .border(2.0, "#ffffff08")
            .child(
                column()
                    .width(width)
                    .height(CARD_HEADER)
                    .justify("end")
                    .pad_bottom(12.0)
                    .child(photo)
                    .child(top_controls),
            )
            .child(
                column()
                    .width(width)
                    .height(CARD_HEIGHT - CARD_HEADER)
                    .bg(CARD_BG)
                    .pad_h(20.0)
                    .pad_v(12.0)
                    .pad_bottom(20.0)
                    .gap(6.0)
                    .child(
                        text(place.name.clone())
                            .size(18.0)
                            .font("brice-bold")
                            .color("#ffffff")
                            .lines(1)
                            .height(22.0),
                    )
                    .child(
                        row()
                            .width(width - 40.0)
                            .height(18.0)
                            .gap(6.0)
                            .align_items("center")
                            .child(
                                text(place.address.clone())
                                    .size(14.0)
                                    .font("satoshi-medium")
                                    .color("#fffffff2")
                                    .width(120.0)
                                    .lines(1)
                                    .height(18.0),
                            )
                            .child(Self::icon("car", 14.0, "#ffffff"))
                            .child(
                                text(format!("{distance} • {drive_minutes} min • {price}"))
                                    .size(14.0)
                                    .font("satoshi-medium")
                                    .color("#fffffff2")
                                    .height(18.0),
                            ),
                    )
                    .child(status)
                    .child(
                        row()
                            .width(150.0)
                            .height(30.0)
                            .pad_h(12.0)
                            .gap(8.0)
                            .radius(15.0)
                            .border(1.0, "#404040")
                            .align_items("center")
                            .child(
                                row()
                                    .width(16.0)
                                    .height(16.0)
                                    .radius(8.0)
                                    .bg("#ffffff")
                                    .align_items("center")
                                    .justify("center")
                                    .child(Self::icon("google", 9.0, "#4285F4")),
                            )
                            .child(
                                text(format!("{} ({})", place.rating, place.reviews))
                                    .size(12.0)
                                    .font("satoshi-medium")
                                    .color("#ffffff")
                                    .width(80.0)
                                    .height(16.0),
                            ),
                    ),
            );
        self.card_cache.borrow_mut().insert(cache_key, card.clone());
        card
    }

    fn carousel(&self, context: &ViewContext, places: &[Place]) -> Element {
        let width = context.viewport.width.max(320.0);
        let card_width = width * 0.90;
        let content_width = width * places.len() as f32;

        let mut strip = hscroll()
            .key("places")
            .absolute()
            .bottom(CAROUSEL_BOTTOM)
            .left(0.0)
            .width(width)
            .height(340.0)
            .paged(true)
            .scroll_x(self.selected as f32 * width)
            .content_width(content_width);

        for (index, place) in places.iter().enumerate() {
            strip = strip.child(
                column()
                    .width(width)
                    .height(340.0)
                    .align_items("center")
                    .justify("end")
                    .child(self.card(card_width, place, index == self.selected)),
            );
        }
        strip
    }

    fn search_bar(&self, _context: &ViewContext) -> Element {
        let results = self.has_results();
        let placeholder = if results {
            "Ask a follow up"
        } else {
            "What are you in the mood for?"
        };

        let mut field = row()
            .weight(1.0)
            .height(SEARCH_HEIGHT)
            .bg(CHIP_BG)
            .border(1.0, "#2d2d2d")
            .radius(36.0)
            .align_items("center")
            .child(
                input(placeholder, self.draft.clone())
                    .key("search")
                    .weight(1.0)
                    .height(64.0)
                    .size(16.0)
                    .font("satoshi-medium")
                    .color("#ffffff")
                    .placeholder_color("#737373")
                    .pad_h(32.0)
                    .align("start"),
            );

        if !self.draft.trim().is_empty() {
            field = field.child(Self::action_button(
                "arrow-up",
                "search:send",
                "#dc2626",
                40.0,
            ));
        }

        let mut bar = row()
            .width(-1.0)
            .height(SEARCH_HEIGHT)
            .gap(12.0)
            .align_items("center")
            .child(field);

        if results {
            bar = bar
                .child(Self::action_button("xmark", "search:clear", CHIP_BG, 48.0).label("Clear"));
        }

        column()
            .width(-1.0)
            .height(SEARCH_HEIGHT)
            .pad_h(24.0)
            .justify("end")
            .test_id("cheetah-search-bar")
            .child(bar)
    }

    fn empty_body(&self) -> Element {
        if let Some(cached) = self.suggestion_cache.borrow().as_ref() {
            return cached.clone();
        }

        let mut suggestions = row()
            .width(600.0)
            .wrap(true)
            .gap(10.0)
            .align_items("center");
        for (index, suggestion) in SUGGESTIONS.iter().enumerate() {
            suggestions = suggestions.child(Self::chip(suggestion, &format!("suggest:{index}")));
        }

        let grid = column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(14.0)
            .child(
                text("Ready to discover?")
                    .size(20.0)
                    .font("brice-semibold")
                    .line_height(26.0)
                    .color(TEXT)
                    .height(26.0),
            )
            .child(
                hscroll()
                    .width(-1.0)
                    .height(260.0)
                    .content_width(600.0)
                    .child(column().width(600.0).height(260.0).child(suggestions)),
            );
        *self.suggestion_cache.borrow_mut() = Some(grid.clone());
        grid
    }

    fn results_body(&self) -> Element {
        let mut body = column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(8.0)
            .child(
                text(self.query.clone())
                    .size(20.0)
                    .font("brice-semibold")
                    .line_height(26.0)
                    .color(TEXT)
                    .height(26.0),
            )
            .child(
                text("Alright, here's what I'd check out if I were you.")
                    .size(16.0)
                    .font("satoshi-medium")
                    .line_height(20.0)
                    .color(MUTED)
                    .height(20.0),
            )
            .child(
                row()
                    .key("show-on-map")
                    .pressable(true)
                    .width(154.0)
                    .height(40.0)
                    .pad_h(16.0)
                    .gap(10.0)
                    .radius(18.0)
                    .bg(CHIP_BG)
                    .border(1.0, "#ffffff14")
                    .align_items("center")
                    .child(
                        text("Show on map")
                            .size(14.0)
                            .font("satoshi-medium")
                            .color("#ffffff")
                            .width(96.0)
                            .height(18.0),
                    )
                    .child(Self::icon("chevron-right", 12.0, "#ffffff")),
            );

        if self.expanded {
            if let Some(response) = self.response() {
                let mut followups = row().width(-1.0).wrap(true).gap(10.0);
                for (index, followup) in response.followups.iter().enumerate() {
                    followups = followups.child(Self::chip(followup, &format!("followup:{index}")));
                }
                body = body.child(followups);
            }
        }

        body
    }

    fn searching_body(&self) -> Element {
        column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(10.0)
            .child(
                text(self.query.clone())
                    .size(20.0)
                    .font("brice-semibold")
                    .color("#ffffff")
                    .height(26.0),
            )
            .child(text("Searching...").size(16.0).height(24.0))
    }

    fn run_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }

        self.search = Resource::Loading;
        self.query = query.into();
        self.draft.clear();
        let result = match &mut self.backend {
            Backend::Local(source) => source.search(query),
            Backend::Remote { client, pending } => {
                // A newer search supersedes any in-flight one.
                if let Some(previous) = pending.take() {
                    self.calls.push(crate::http::cancel(previous));
                }
                let request = client
                    .post("/v1/search")
                    .json(&serde_json::json!({ "text": query }));
                *pending = Some(request.id);
                self.calls.push(request.into_call());
                return;
            }
        };
        self.finish_search(result);
    }

    fn finish_search(&mut self, result: Result<SearchResponse, String>) {
        match result {
            Ok(response) => {
                self.search = Resource::Ready(response);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
                self.haptic("light");
            }
            Err(error) => {
                self.search = Resource::Failed(error);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
            }
        }
    }

    fn clear_search(&mut self) {
        if let Backend::Remote { pending, .. } = &mut self.backend {
            if let Some(id) = pending.take() {
                self.calls.push(crate::http::cancel(id));
            }
        }
        self.query.clear();
        self.draft.clear();
        self.search = Resource::Idle;
        self.selected = 0;
        self.focused = false;
        self.expanded = false;
    }

    fn select_place(&mut self, id: &str) {
        let Some(response) = self.response() else {
            return;
        };
        if let Some(index) = response.places.iter().position(|place| place.id == id) {
            self.selected = index;
            self.focused = true;
            self.haptic("selection");
        }
    }

    fn haptic(&mut self, style: &str) {
        self.calls.push(NativeCall::new("haptics", style, "{}"));
    }

    fn handle_native_event(&mut self, payload: &str) {
        let Some(event) = NativeEvent::parse(payload) else {
            return;
        };

        if let Some(response) = HttpResponse::from_event(&event) {
            let Backend::Remote { pending, .. } = &mut self.backend else {
                return;
            };
            // Ignore cancelled or superseded requests.
            if *pending != Some(response.id) {
                return;
            }
            *pending = None;
            self.finish_search(parse_search(&response));
            return;
        }

        if !event.is("location", "result") || event.data["ok"] != true {
            return;
        }
        let latitude = event.data["latitude"].as_f64();
        let longitude = event.data["longitude"].as_f64();
        if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
            self.center = Some((latitude as f32, longitude as f32));
        }
    }

    fn sheet(&self, context: &ViewContext) -> Element {
        let height = self.sheet_height(context);
        let body_height = (height - 24.0 - SEARCH_HEIGHT - SEARCH_BOTTOM_GAP).max(0.0);
        let body = if self.has_results() && !self.expanded {
            column().width(-1.0).height(body_height)
        } else {
            match &self.search {
                Resource::Ready(_) => self.results_body(),
                Resource::Loading => self.searching_body(),
                _ => self.empty_body(),
            }
        };

        sheet()
            .key("search-panel")
            .absolute()
            .bottom(0.0)
            .left(0.0)
            .width(-1.0)
            .height(height)
            .radius(54.0)
            .clip(true)
            .bg(SHEET_BG)
            .child(
                column()
                    .width(-1.0)
                    .height(height)
                    .child(
                        row()
                            .key("sheet:toggle")
                            .pressable(true)
                            .width(-1.0)
                            .height(24.0)
                            .justify("center")
                            .align_items("center")
                            .child(row().width(30.0).height(6.0).bg("#404040").radius(3.0)),
                    )
                    .child(
                        column()
                            .width(-1.0)
                            .height(body_height)
                            .clip(true)
                            .child(body),
                    )
                    .child(self.search_bar(context))
                    .child(column().width(-1.0).height(SEARCH_BOTTOM_GAP)),
            )
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
}

#[cfg(test)]
mod tests {
    use super::{clock_time, Backend, NightshadeApp};
    use crate::app::{App, Event, Runtime, Viewport};
    use crate::diff::Op;
    use crate::resource::Resource;

    fn click(app: &mut NightshadeApp, key: &str) {
        app.update(Event {
            id: 0,
            key: Some(key.into()),
            kind: "click".into(),
            payload: String::new(),
        });
    }

    fn select(app: &mut NightshadeApp, key: &str, value: &str) {
        app.update(Event {
            id: 0,
            key: Some(key.into()),
            kind: "select".into(),
            payload: value.into(),
        });
    }

    #[test]
    fn starts_without_results() {
        let app = NightshadeApp::new(false);
        assert!(!app.has_results());
        assert_eq!(app.response(), None);
    }

    #[test]
    fn native_menu_runs_a_search() {
        let mut app = NightshadeApp::new(false);
        select(&mut app, "open-menu", "Sushi");

        let response = app.response().expect("ready response");
        assert_eq!(response.places.len(), 1);
        assert_eq!(response.places[0].id, "sushi");
    }

    #[test]
    fn suggestion_runs_a_search() {
        let mut app = NightshadeApp::new(false);
        click(&mut app, "suggest:0");

        assert!(app.has_results());
        assert_eq!(app.query, "Cafes within 12 min walk");
    }

    #[test]
    fn submit_search_and_select_card() {
        let mut app = NightshadeApp::new(false);
        app.update(Event {
            id: 0,
            key: Some("search".into()),
            kind: "change".into(),
            payload: "sushi".into(),
        });
        app.update(Event {
            id: 0,
            key: Some("search".into()),
            kind: "submit".into(),
            payload: String::new(),
        });
        let response = app.response().expect("ready response");
        assert_eq!(response.places.len(), 1);
        assert_eq!(response.places[0].id, "sushi");

        click(&mut app, "place:sushi");
        assert_eq!(app.selected, 0);
        click(&mut app, "search:clear");
        assert!(!app.has_results());
        assert_eq!(app.response(), None);
    }

    #[test]
    fn failed_search_is_a_typed_state() {
        let mut app = NightshadeApp::new(false);
        click(&mut app, "suggest:0");
        app.draft = "fail".into();
        click(&mut app, "search:send");

        assert!(!app.has_results());
        assert_eq!(app.search.error(), Some("Place search is unavailable"));
    }

    fn remote_app() -> NightshadeApp {
        let mut app = NightshadeApp::with_backend(false, Backend::remote("http://api.test"));
        app.take_native_calls();
        app
    }

    fn submit(app: &mut NightshadeApp, query: &str) {
        app.draft = query.into();
        click(app, "search:send");
    }

    fn http_response(app: &mut NightshadeApp, payload: serde_json::Value) {
        let mut payload = payload;
        payload["module"] = "http".into();
        payload["method"] = "response".into();
        app.update(Event {
            id: 0,
            key: None,
            kind: "native".into(),
            payload: payload.to_string(),
        });
    }

    fn request_id(app: &mut NightshadeApp) -> u64 {
        let calls = app.take_native_calls();
        let call = calls
            .iter()
            .find(|call| call.module == "http" && call.method == "request")
            .expect("http request");
        let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
        assert_eq!(args["url"], "http://api.test/v1/search");
        assert_eq!(args["method"], "POST");
        assert_eq!(args["timeoutMs"], 60_000);
        args["id"].as_u64().unwrap()
    }

    #[test]
    fn remote_search_posts_text_and_maps_cards() {
        let mut app = remote_app();
        submit(&mut app, "halal pizza");
        assert!(app.search.is_loading());
        assert_eq!(app.query, "halal pizza");

        let calls = app.take_native_calls();
        let call = calls.iter().find(|call| call.module == "http").unwrap();
        let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
        assert_eq!(args["body"], r#"{"text":"halal pizza"}"#);
        let id = args["id"].as_u64().unwrap();

        let body = serde_json::json!({
            "title": "Results",
            "cards": [{
                "place_id": "p1", "name": "Luigi's \"Best\" Pizza", "address": null,
                "lat": 45.4, "lng": -75.7, "distance_m": 950, "price_tier": 2,
                "open_now": true, "closes_at_local": "2026-11-15T22:30:00",
                "quality_score": 0.8, "hours": {"mon": []}
            }, {
                "place_id": "p2", "name": "Mystery Hours", "lat": 45.41, "lng": -75.71,
                "distance_m": 0, "open_now": null
            }],
            "pins": [], "followups": ["open now"], "mode": "llm_new", "warnings": []
        });
        http_response(
            &mut app,
            serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}),
        );

        let response = app.response().expect("ready");
        assert_eq!(response.places.len(), 2);
        let first = &response.places[0];
        assert_eq!(first.id, "p1");
        assert_eq!(first.address, "");
        assert_eq!(first.open, Some(true));
        assert_eq!(first.closes, "10:30 PM");
        assert_eq!(response.places[1].open, None);
        assert_eq!(response.followups, vec!["open now".to_string()]);

        // Names with quotes must still produce valid marker JSON.
        let markers: serde_json::Value = serde_json::from_str(&app.markers()).unwrap();
        assert_eq!(markers[0]["name"], "Luigi's \"Best\" Pizza");
    }

    #[test]
    fn newer_search_cancels_and_ignores_the_stale_response() {
        let mut app = remote_app();
        submit(&mut app, "sushi");
        let first = request_id(&mut app);
        submit(&mut app, "pizza");
        let calls = app.take_native_calls();
        assert!(calls
            .iter()
            .any(|call| call.module == "http" && call.method == "cancel"));

        http_response(
            &mut app,
            serde_json::json!({"id": first, "ok": true, "status": 200, "body": "{\"cards\":[]}"}),
        );
        assert!(app.search.is_loading());
    }

    #[test]
    fn remote_failures_become_typed_errors() {
        let mut app = remote_app();
        submit(&mut app, "sushi");
        let id = request_id(&mut app);
        http_response(
            &mut app,
            serde_json::json!({"id": id, "ok": false, "error": "timeout"}),
        );
        assert_eq!(app.search, Resource::Failed("Search timed out".into()));

        submit(&mut app, "sushi");
        let id = request_id(&mut app);
        http_response(
            &mut app,
            serde_json::json!({"id": id, "ok": true, "status": 500, "body": "{}"}),
        );
        assert_eq!(app.search, Resource::Failed("Search failed (500)".into()));

        submit(&mut app, "sushi");
        let id = request_id(&mut app);
        http_response(
            &mut app,
            serde_json::json!({"id": id, "ok": true, "status": 200, "body": "<html>"}),
        );
        assert_eq!(
            app.search,
            Resource::Failed("Unexpected search response".into())
        );
    }

    #[test]
    fn clock_time_formats_backend_times() {
        assert_eq!(
            clock_time("2026-11-15T22:30:00").as_deref(),
            Some("10:30 PM")
        );
        assert_eq!(clock_time("00:05").as_deref(), Some("12:05 AM"));
        assert_eq!(clock_time("12:00:00-05:00").as_deref(), Some("12:00 PM"));
        assert_eq!(clock_time("late"), None);
    }

    #[test]
    fn native_location_recenters_and_calls_reach_the_wire() {
        let mut app = NightshadeApp::new(false);
        let startup = app.take_native_calls();
        assert!(startup
            .iter()
            .any(|call| call.module == "location" && call.method == "request"));

        app.update(Event {
            id: 0,
            key: None,
            kind: "native".into(),
            payload: r#"{"module":"location","method":"result","ok":true,"latitude":40.0,"longitude":-73.0}"#.into(),
        });
        assert_eq!(app.center, Some((40.0, -73.0)));

        click(&mut app, "suggest:0");
        let mut runtime = Runtime::new(app);
        let ops = runtime.render(Viewport {
            width: 393.0,
            height: 852.0,
        });
        assert!(ops.iter().any(|op| {
            matches!(
                op,
                Op::Invoke { module, method, .. }
                    if module == "haptics" && method == "light"
            )
        }));
    }
}
