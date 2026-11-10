use crate::app::{App, Event, HostInfo, ViewContext};
use crate::element::{column, glass, hscroll, image, input, map, row, sheet, text, Element};
use crate::resource::Resource;

const BG: &str = "#0b0b0f";
const CHIP: &str = "#292524";
const TEXT: &str = "#f5f5f5";
const MUTED: &str = "#a3a3a3";
const SHEET_HEIGHT: f32 = 230.0;

const INITIAL_LAT: f32 = 37.7749;
const INITIAL_LNG: f32 = -122.4194;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    id: &'static str,
    name: &'static str,
    address: &'static str,
    detail: &'static str,
    lat: f32,
    lng: f32,
    open: bool,
}

const PLACES: [Place; 4] = [
    Place {
        id: "java",
        name: "Java Haven",
        address: "Cedarwood Drive",
        detail: "Open until 6:00 PM",
        lat: 37.7793,
        lng: -122.4192,
        open: true,
    },
    Place {
        id: "pizza",
        name: "Pizza Paradise",
        address: "Market Street",
        detail: "Open until 11:00 PM",
        lat: 37.7849,
        lng: -122.4094,
        open: true,
    },
    Place {
        id: "sushi",
        name: "Sushi Zen",
        address: "Hayes Valley",
        detail: "Open until 9:30 PM",
        lat: 37.7694,
        lng: -122.4862,
        open: true,
    },
    Place {
        id: "burger",
        name: "Burger Joint",
        address: "Mission Street",
        detail: "Open until 10:00 PM",
        lat: 37.7599,
        lng: -122.4148,
        open: false,
    },
];

#[derive(Clone, Debug, PartialEq)]
struct SearchResponse {
    places: Vec<Place>,
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
        } else if lowered.contains("coffee") || lowered.contains("cafe") {
            vec![PLACES[0]]
        } else if lowered.contains("open") {
            PLACES.iter().copied().filter(|place| place.open).collect()
        } else {
            PLACES.to_vec()
        };

        Ok(SearchResponse { places })
    }
}

pub struct NightshadeApp {
    host: HostInfo,
    draft: String,
    query: String,
    search: Resource<SearchResponse>,
    source: Box<dyn PlaceSource>,
    selected: usize,
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
        };

        if demo {
            app.search = Resource::Ready(SearchResponse {
                places: PLACES.to_vec(),
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

    fn chip(label: &str, width: f32, key: &str) -> Element {
        text(label)
            .key(key)
            .pressable(true)
            .size(13.0)
            .color("#d4d4d4")
            .bg(CHIP)
            .width(width)
            .height(34.0)
            .align("center")
            .radius(17.0)
    }

    fn card(&self, width: f32, place: Place, selected: bool) -> Element {
        let background = if selected { "#33201c" } else { "#1f1f1f" };
        column()
            .key(format!("place-{}", place.id))
            .pressable(true)
            .width(width)
            .height(276.0)
            .bg(background)
            .radius(28.0)
            .child(
                image("asset://second-cup-card-full.jpeg")
                    .width(-1.0)
                    .height(168.0)
                    .radius(28.0),
            )
            .child(
                column()
                    .width(-1.0)
                    .height(108.0)
                    .pad_h(18.0)
                    .pad_v(14.0)
                    .gap(5.0)
                    .child(
                        text(place.name)
                            .size(18.0)
                            .bold(true)
                            .color(TEXT)
                            .height(22.0),
                    )
                    .child(text(place.address).size(13.0).color("#d4d4d4").height(18.0))
                    .child(text(place.detail).size(12.0).color(MUTED).height(16.0)),
            )
    }

    fn carousel(&self, width: f32, places: &[Place]) -> Element {
        let card_width = (width * 0.82).min(340.0);
        let content_width = (card_width + 12.0) * places.len() as f32 + 40.0;
        let mut scroll = hscroll()
            .key("places")
            .absolute()
            .bottom(self.host.safe_bottom.max(0.0) + 12.0 + SHEET_HEIGHT + 12.0)
            .left(0.0)
            .width(-1.0)
            .height(300.0)
            .pad_h(16.0)
            .gap(12.0)
            .content_width(content_width);

        for (index, place) in places.iter().copied().enumerate() {
            scroll = scroll.child(self.card(card_width, place, index == self.selected));
        }
        scroll
    }

    fn bottom_panel(&self, _width: f32) -> Element {
        let results = self.has_results();
        let placeholder = if results {
            "Ask a follow up"
        } else {
            "What are you in the mood for?"
        };
        let mut chips = row().width(-1.0).height(34.0).gap(8.0);
        if results {
            chips = chips
                .child(Self::chip("Coffee", 64.0, "query:coffee"))
                .child(Self::chip("Sushi", 64.0, "query:sushi"))
                .child(Self::chip("Pizza", 64.0, "query:pizza"))
                .child(Self::chip("Clear", 64.0, "search:clear"));
        } else {
            chips = chips
                .child(Self::chip("Coffee", 72.0, "query:coffee"))
                .child(Self::chip("Sushi", 72.0, "query:sushi"))
                .child(Self::chip("Pizza", 72.0, "query:pizza"))
                .child(Self::chip("Open now", 92.0, "query:open"));
        }

        let footer = match &self.search {
            Resource::Ready(_) => format!("Results for \"{}\" · Cedarwood Drive", self.query),
            Resource::Failed(error) => format!("Search failed: {error}"),
            _ => "Nightshade · Cedarwood Drive".into(),
        };

        sheet()
            .key("search-panel")
            .absolute()
            .bottom(0.0)
            .left(0.0)
            .width(-1.0)
            .height(SHEET_HEIGHT)
            .radius(28.0)
            .child(
                column()
                    .width(-1.0)
                    .height(-1.0)
                    .pad(16.0)
                    .gap(12.0)
                    .child(
                        row()
                            .width(-1.0)
                            .height(6.0)
                            .justify("center")
                            .child(row().width(36.0).height(4.0).bg("#404040").radius(2.0)),
                    )
                    .child(
                        input(placeholder, self.draft.clone())
                            .key("search")
                            .width(-1.0)
                            .height(52.0)
                            .size(16.0)
                            .color(TEXT)
                            .bg("#242424")
                            .pad_h(20.0)
                            .align("start")
                            .radius(26.0),
                    )
                    .child(chips)
                    .child(text(footer).size(12.0).color(MUTED).height(16.0)),
            )
    }

    fn top_control(&self) -> Element {
        glass()
            .key("location")
            .glass_style("regular")
            .glass_tint("#171717cc")
            .absolute()
            .top(self.host.safe_top.max(0.0) + 12.0)
            .left(12.0)
            .width(220.0)
            .height(46.0)
            .radius(23.0)
            .child(
                row()
                    .width(-1.0)
                    .height(-1.0)
                    .pad_h(16.0)
                    .gap(8.0)
                    .align_items("center")
                    .child(
                        text("Nearby")
                            .size(14.0)
                            .bold(true)
                            .color(TEXT)
                            .width(54.0)
                            .height(20.0),
                    )
                    .child(
                        text("Cedarwood Drive")
                            .size(13.0)
                            .color(MUTED)
                            .weight(1.0)
                            .height(20.0),
                    ),
            )
    }

    fn visible_places(&self) -> &[Place] {
        self.response()
            .map(|response| response.places.as_slice())
            .unwrap_or(&PLACES)
    }

    fn markers(places: &[Place]) -> String {
        let items = places
            .iter()
            .map(|place| {
                format!(
                    r#"{{"name":"{}","lat":{},"lng":{}}}"#,
                    place.name, place.lat, place.lng
                )
            })
            .collect::<Vec<_>>();
        format!("[{}]", items.join(","))
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
            }
            Err(error) => {
                self.search = Resource::Failed(error);
                self.query = query.into();
                self.selected = 0;
            }
        }
        self.draft.clear();
    }

    fn clear_search(&mut self) {
        self.query.clear();
        self.draft.clear();
        self.search = Resource::Idle;
        self.selected = 0;
    }

    fn select_place(&mut self, id: &str) {
        let Some(response) = self.response() else {
            return;
        };
        if let Some(index) = response.places.iter().position(|place| place.id == id) {
            self.selected = index;
        }
    }
}

impl Default for NightshadeApp {
    fn default() -> Self {
        Self::new(false)
    }
}

impl App for NightshadeApp {
    fn view(&self, context: &ViewContext) -> Element {
        let width = context.viewport.width.max(320.0);
        let visible = self.visible_places();
        let selected = self
            .response()
            .and_then(|response| response.places.get(self.selected).copied());
        let (latitude, longitude, delta) = selected
            .map(|place| (place.lat, place.lng, 0.018))
            .unwrap_or((INITIAL_LAT, INITIAL_LNG, 0.025));

        let mut root = column()
            .width(-1.0)
            .height(-1.0)
            .bg(BG)
            .child(
                map()
                    .width(-1.0)
                    .height(-1.0)
                    .bg("#101826")
                    .test_id("nightshade-map")
                    .map_region(latitude, longitude, delta, delta)
                    .map_markers(Self::markers(visible))
                    .map_dark(true),
            )
            .child(self.top_control());

        if self.has_results() {
            root = root.child(self.carousel(width, visible));
        }

        root.child(self.bottom_panel(width))
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
            "click" => {
                if let Some(query) = key.strip_prefix("query:") {
                    self.run_search(query);
                } else if key == "search:clear" {
                    self.clear_search();
                } else if let Some(id) = key.strip_prefix("place:") {
                    self.select_place(id);
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

    #[test]
    fn starts_without_results() {
        let app = NightshadeApp::new(false);
        assert!(!app.has_results());
        assert_eq!(app.response(), None);
    }

    #[test]
    fn chip_search_filters_places_and_clears_draft() {
        let mut app = NightshadeApp::new(false);
        app.draft = "unused".into();
        click(&mut app, "query:coffee");

        assert!(app.has_results());
        let response = app.response().expect("ready response");
        assert_eq!(response.places.len(), 1);
        assert_eq!(response.places[0].id, "java");
        assert!(app.draft.is_empty());
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
        click(&mut app, "query:fail");

        assert!(!app.has_results());
        assert_eq!(app.search.error(), Some("Place search is unavailable"));
    }
}
