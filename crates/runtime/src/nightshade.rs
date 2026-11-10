use crate::app::{App, Event, HostInfo, ViewContext};
use crate::element::{column, glass, hscroll, image, input, map, row, text, Element};

const BG: &str = "#0b0b0f";
const PANEL: &str = "#171717e6";
const CHIP: &str = "#292524";
const TEXT: &str = "#f5f5f5";
const MUTED: &str = "#a3a3a3";

const MARKERS: &str = r#"[
    {"name":"Java Haven","lat":37.7793,"lng":-122.4192},
    {"name":"Pizza Paradise","lat":37.7849,"lng":-122.4094},
    {"name":"Sushi Zen","lat":37.7694,"lng":-122.4862},
    {"name":"Burger Joint","lat":37.7599,"lng":-122.4148}
]"#;

pub struct NightshadeApp {
    host: HostInfo,
}

impl NightshadeApp {
    pub fn new() -> Self {
        Self {
            host: HostInfo::default(),
        }
    }

    fn chip(label: &str, width: f32) -> Element {
        text(label)
            .size(13.0)
            .color("#d4d4d4")
            .bg(CHIP)
            .width(width)
            .height(34.0)
            .align("center")
            .radius(17.0)
    }

    fn card(&self, width: f32, name: &str, address: &str, detail: &str) -> Element {
        column()
            .width(width)
            .height(276.0)
            .bg("#1f1f1f")
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
                    .child(text(name).size(18.0).bold(true).color(TEXT).height(22.0))
                    .child(text(address).size(13.0).color("#d4d4d4").height(18.0))
                    .child(text(detail).size(12.0).color(MUTED).height(16.0)),
            )
    }

    fn carousel(&self, width: f32) -> Element {
        let card_width = (width * 0.82).min(340.0);
        let content_width = (card_width + 12.0) * 3.0 + 40.0;
        hscroll()
            .key("places")
            .absolute()
            .bottom(self.host.safe_bottom.max(0.0) + 12.0 + 166.0 + 12.0)
            .left(0.0)
            .width(-1.0)
            .height(300.0)
            .pad_h(16.0)
            .gap(12.0)
            .content_width(content_width)
            .child(self.card(
                card_width,
                "Java Haven",
                "Cedarwood Drive",
                "Open until 6:00 PM",
            ))
            .child(self.card(
                card_width,
                "Pizza Paradise",
                "Market Street",
                "Open until 11:00 PM",
            ))
            .child(self.card(
                card_width,
                "Sushi Zen",
                "Hayes Valley",
                "Open until 9:30 PM",
            ))
    }

    fn bottom_panel(&self, width: f32) -> Element {
        let panel_width = (width - 24.0).max(280.0);
        glass()
            .key("search-panel")
            .glass_style("regular")
            .glass_tint(PANEL)
            .absolute()
            .bottom(self.host.safe_bottom.max(0.0) + 12.0)
            .left(12.0)
            .width(panel_width)
            .height(166.0)
            .radius(28.0)
            .child(
                column()
                    .width(-1.0)
                    .height(-1.0)
                    .pad(16.0)
                    .gap(12.0)
                    .child(
                        input("What are you in the mood for?", "")
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
                    .child(
                        row()
                            .width(-1.0)
                            .height(34.0)
                            .gap(8.0)
                            .child(Self::chip("Coffee", 72.0))
                            .child(Self::chip("Sushi", 72.0))
                            .child(Self::chip("Pizza", 72.0))
                            .child(Self::chip("Open now", 92.0)),
                    )
                    .child(
                        text("Nightshade · Cedarwood Drive")
                            .size(12.0)
                            .color(MUTED)
                            .height(16.0),
                    ),
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
}

impl Default for NightshadeApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for NightshadeApp {
    fn view(&self, context: &ViewContext) -> Element {
        let width = context.viewport.width.max(320.0);
        column()
            .width(-1.0)
            .height(-1.0)
            .bg(BG)
            .child(
                map()
                    .width(-1.0)
                    .height(-1.0)
                    .bg("#101826")
                    .test_id("nightshade-map")
                    .map_region(37.7749, -122.4194, 0.025, 0.025)
                    .map_markers(MARKERS)
                    .map_dark(true),
            )
            .child(self.top_control())
            .child(self.carousel(width))
            .child(self.bottom_panel(width))
    }

    fn update(&mut self, event: Event) {
        if event.kind == "host" {
            if let Ok(host) = serde_json::from_str::<HostInfo>(&event.payload) {
                self.host = host;
            }
        }
    }
}
