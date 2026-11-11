use crate::app::{App, Event, HostInfo, ViewContext};
use crate::element::{
    column, glass, hscroll, image, input, map, native, row, sheet, text, Element,
};
use crate::resource::Resource;

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    id: &'static str,
    name: &'static str,
    address: &'static str,
    lat: f32,
    lng: f32,
    distance_m: f32,
    price_tier: u8,
    open: bool,
    closes: &'static str,
    opens: &'static str,
    rating: &'static str,
    reviews: &'static str,
    cuisine: &'static str,
    icon: &'static str,
    icon_color: &'static str,
}

const PLACES: [Place; 4] = [
    Place {
        id: "java",
        name: "Java Haven",
        address: "Cedarwood Drive",
        lat: 37.7793,
        lng: -122.4192,
        distance_m: 280.0,
        price_tier: 2,
        open: true,
        closes: "6:00 PM",
        opens: "7:00 AM",
        rating: "4.8",
        reviews: "120+",
        cuisine: "Coffee",
        icon: "coffee",
        icon_color: "#7B3F00",
    },
    Place {
        id: "pizza",
        name: "Pizza Paradise",
        address: "Market Street",
        lat: 37.7849,
        lng: -122.4094,
        distance_m: 1_200.0,
        price_tier: 2,
        open: true,
        closes: "11:00 PM",
        opens: "11:00 AM",
        rating: "4.2",
        reviews: "80+",
        cuisine: "Pizza",
        icon: "pizza-slice",
        icon_color: "#FFA500",
    },
    Place {
        id: "sushi",
        name: "Sushi Zen",
        address: "Hayes Valley",
        lat: 37.7694,
        lng: -122.4862,
        distance_m: 700.0,
        price_tier: 3,
        open: true,
        closes: "9:30 PM",
        opens: "11:30 AM",
        rating: "4.6",
        reviews: "50+",
        cuisine: "Sushi",
        icon: "fish",
        icon_color: "#008080",
    },
    Place {
        id: "burger",
        name: "Burger Joint",
        address: "Mission Street",
        lat: 37.7599,
        lng: -122.4148,
        distance_m: 520.0,
        price_tier: 1,
        open: false,
        closes: "10:00 PM",
        opens: "10:30 AM",
        rating: "4.1",
        reviews: "90+",
        cuisine: "Fast Food",
        icon: "hamburger",
        icon_color: "#FF4500",
    },
];

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

        let places = if lowered.contains("sushi") {
            vec![PLACES[2]]
        } else if lowered.contains("pizza") {
            vec![PLACES[1]]
        } else if lowered.contains("coffee")
            || lowered.contains("cafe")
            || lowered.contains("brunch")
        {
            vec![PLACES[0]]
        } else if lowered.contains("burger") || lowered.contains("fast") {
            vec![PLACES[3]]
        } else if lowered.contains("open") || lowered.contains("late") {
            PLACES.iter().copied().filter(|place| place.open).collect()
        } else {
            PLACES.to_vec()
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

pub struct NightshadeApp {
    host: HostInfo,
    draft: String,
    query: String,
    search: Resource<SearchResponse>,
    source: Box<dyn PlaceSource>,
    selected: usize,
    focused: bool,
}

impl NightshadeApp {
    pub fn new(demo: bool) -> Self {
        let mut app = Self {
            host: HostInfo::default(),
            draft: String::new(),
            query: String::new(),
            search: Resource::Idle,
            source: Box::new(LocalPlaceSource),
            selected: 0,
            focused: false,
        };

        if demo {
            app.search = Resource::Ready(SearchResponse {
                places: PLACES.to_vec(),
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
        if self.has_results() {
            RESULT_SHEET_HEIGHT
        } else {
            (context.viewport.height * 0.52).max(390.0)
        }
    }

    fn visible_places(&self) -> &[Place] {
        self.response()
            .map(|response| response.places.as_slice())
            .unwrap_or(&PLACES)
    }

    fn region(&self) -> (f32, f32, f32, f32) {
        if !self.has_results() {
            return (INITIAL_LAT, INITIAL_LNG, 0.025, 0.025);
        }

        let places = self.visible_places();
        let Some(first) = places.first() else {
            return (INITIAL_LAT, INITIAL_LNG, 0.025, 0.025);
        };

        if self.focused || places.len() == 1 {
            let place = places.get(self.selected).copied().unwrap_or(*first);
            let delta = 0.018;
            return (place.lat - delta * 0.32, place.lng, delta, delta);
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
        let latitude_delta = ((max_lat - min_lat) * 2.4).max(0.015);
        let longitude_delta = ((max_lng - min_lng) * 2.4).max(0.015);
        (latitude, longitude, latitude_delta, longitude_delta)
    }

    fn markers(&self) -> String {
        let results = self.has_results();
        let items = self
            .visible_places()
            .iter()
            .enumerate()
            .map(|(index, place)| {
                format!(
                    r#"{{"id":"{}","name":"{}","lat":{},"lng":{},"kind":"{}","icon":"{}","iconColor":"{}","selected":{}}}"#,
                    place.id,
                    place.name,
                    place.lat,
                    place.lng,
                    if results { "result" } else { "static" },
                    if results { "utensils" } else { place.icon },
                    if results { "#ffffff" } else { place.icon_color },
                    results && index == self.selected,
                )
            })
            .collect::<Vec<_>>();
        format!("[{}]", items.join(","))
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
            .glass_tint("#00000040")
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

    fn dot(active: bool) -> Element {
        column()
            .width(if active { 8.0 } else { 5.0 })
            .height(if active { 8.0 } else { 5.0 })
            .radius(4.0)
            .bg("#ffffff")
            .opacity(if active { 1.0 } else { 0.22 })
    }

    fn card(&self, width: f32, place: Place, _selected: bool) -> Element {
        let drive_minutes = ((place.distance_m / 1000.0) / 30.0 * 60.0).round().max(1.0) as u32;
        let distance = if place.distance_m < 1000.0 {
            format!("{}m", place.distance_m.round() as u32)
        } else {
            format!("{:.1}km", place.distance_m / 1000.0)
        };
        let price = "$".repeat(place.price_tier.min(4) as usize);
        let status = if place.open {
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
                    text("-")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(format!("Closes at {}", place.closes))
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
                    text("-")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(format!("Opens at {}", place.opens))
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

        let dots = row()
            .width(width)
            .height(8.0)
            .justify("center")
            .align_items("center")
            .gap(6.0)
            .child(Self::dot(true))
            .child(Self::dot(false))
            .child(Self::dot(false))
            .child(Self::dot(false))
            .child(Self::dot(false));

        let gradient = native("gradient")
            .absolute()
            .bottom(0.0)
            .left(0.0)
            .width(width)
            .height(45.0)
            .native_props(
                r##"{"colors":["#0b0b0f00","#0b0b0f59","#0b0b0fb3","#1f1f1f"],"locations":[0,0.5,0.8,1],"startX":0,"startY":0,"endX":0,"endY":1}"##,
            );

        column()
            .key(format!("place-{}", place.id))
            .pressable(true)
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
                    .child(
                        image("asset://second-cup-card-full.jpeg")
                            .absolute()
                            .top(0.0)
                            .left(0.0)
                            .width(width)
                            .height(CARD_HEADER),
                    )
                    .child(gradient)
                    .child(top_controls)
                    .child(dots),
            )
            .child(
                column()
                    .width(width)
                    .height(CARD_HEIGHT - CARD_HEADER)
                    .pad_h(20.0)
                    .pad_v(12.0)
                    .gap(6.0)
                    .child(
                        text(place.name)
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
                                text(place.address)
                                    .size(14.0)
                                    .font("satoshi-medium")
                                    .color("#fffffff2")
                                    .weight(1.0)
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
                                    .height(16.0),
                            ),
                    ),
            )
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

        for (index, place) in places.iter().copied().enumerate() {
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
                    .pad_h(20.0)
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
            .child(bar)
    }

    fn empty_body(&self) -> Element {
        let mut suggestions = row().width(-1.0).wrap(true).gap(10.0).align_items("center");
        for (index, suggestion) in SUGGESTIONS.iter().enumerate() {
            suggestions = suggestions.child(Self::chip(suggestion, &format!("suggest:{index}")));
        }

        column()
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
            .child(suggestions)
    }

    fn results_body(&self) -> Element {
        let body = column()
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

        if let Some(response) = self.response() {
            // Follow-ups belong to the expanded state in the native sheet.
            let _ = response.followups.len();
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
        match self.source.search(query) {
            Ok(response) => {
                self.search = Resource::Ready(response);
                self.query = query.into();
                self.selected = 0;
                self.focused = false;
            }
            Err(error) => {
                self.search = Resource::Failed(error);
                self.query = query.into();
                self.selected = 0;
                self.focused = false;
            }
        }
        self.draft.clear();
    }

    fn clear_search(&mut self) {
        self.query.clear();
        self.draft.clear();
        self.search = Resource::Idle;
        self.selected = 0;
        self.focused = false;
    }

    fn select_place(&mut self, id: &str) {
        let Some(response) = self.response() else {
            return;
        };
        if let Some(index) = response.places.iter().position(|place| place.id == id) {
            self.selected = index;
            self.focused = true;
        }
    }

    fn sheet(&self, context: &ViewContext) -> Element {
        let height = self.sheet_height(context);
        let body_height = (height - 24.0 - SEARCH_HEIGHT).max(0.0);
        let body = match &self.search {
            Resource::Ready(_) => self.results_body(),
            Resource::Loading => self.searching_body(),
            _ => self.empty_body(),
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
                    .child(self.search_bar(context)),
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

        let Some(key) = event.key.as_deref() else {
            return;
        };

        match event.kind.as_str() {
            "change" if key == "search" => {
                self.draft = event.payload;
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
                } else if key == "search:send" {
                    let query = self.draft.clone();
                    self.run_search(&query);
                } else if key == "search:clear" {
                    self.clear_search();
                } else if key == "show-on-map" {
                    // The sheet is already collapsed in the result state.
                } else if let Some(id) = key.strip_prefix("place:") {
                    self.select_place(id);
                } else if key.starts_with("card:") {
                    // Card actions are visual for now.
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NightshadeApp;
    use crate::app::{App, Event};

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
}
