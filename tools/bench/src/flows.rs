//! Nightshade's own flows through the runtime: what a person does in the app,
//! with a full page of results, against the same views device builds use.

use crate::nightshade::NightshadeApp;
use crate::{elapsed_ms, HOST_INFO};
use runtime::{Event, Op, Runtime, Viewport};
use serde_json::{json, Value};
use std::time::Instant;

pub const RUNS: usize = 30;
const CARDS: usize = 12;

/// What a person feels: how soon the app is there, how soon a tap shows, and
/// whether scrolling keeps up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Group {
    Startup,
    ViewChange,
    Scroll,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Startup, Group::ViewChange, Group::Scroll];

    pub fn name(self) -> &'static str {
        match self {
            Group::Startup => "startup",
            Group::ViewChange => "view change",
            Group::Scroll => "scroll",
        }
    }

    /// The runtime's share of a frame, for p95. A 120 Hz frame is 8.3 ms and
    /// UIKit needs most of it to draw, so a view change may take one whole
    /// frame and scrolling, which repeats every frame, a quarter of one.
    /// Startup may take one 60 Hz frame.
    pub fn budget_ms(self) -> f64 {
        match self {
            Group::Startup => 16.7,
            Group::ViewChange => 8.3,
            Group::Scroll => 4.0,
        }
    }
}

/// One step's timings and op traffic, over every run.
pub struct Flow {
    pub name: &'static str,
    pub group: Group,
    pub ms: Vec<f64>,
    pub ops: Vec<usize>,
    pub bytes: Vec<usize>,
    pub nodes: usize,
    pub build_us: Vec<u64>,
    pub diff_us: Vec<u64>,
    pub layout_us: Vec<u64>,
}

struct Session {
    runtime: Runtime,
    flows: Vec<Flow>,
    /// The flow list position of the current step.
    step: usize,
}

impl Session {
    fn record(&mut self, group: Group, name: &'static str, run: impl FnOnce(&mut Runtime) -> Vec<Op>) -> Vec<Op> {
        let start = Instant::now();
        let ops = run(&mut self.runtime);
        let bytes = runtime::encode_ops(&ops).len();
        let ms = elapsed_ms(start);
        if self.step == self.flows.len() {
            self.flows.push(Flow {
                name,
                group,
                ms: Vec::new(),
                ops: Vec::new(),
                bytes: Vec::new(),
                nodes: 0,
                build_us: Vec::new(),
                diff_us: Vec::new(),
                layout_us: Vec::new(),
            });
        }
        let metrics = self.runtime.metrics();
        let flow = &mut self.flows[self.step];
        assert_eq!(flow.name, name, "every run takes the same steps");
        flow.ms.push(ms);
        flow.ops.push(ops.len());
        flow.bytes.push(bytes);
        flow.nodes = metrics.nodes;
        flow.build_us.push(metrics.build_us);
        flow.diff_us.push(metrics.diff_us);
        flow.layout_us.push(metrics.layout_us);
        self.step += 1;
        ops
    }

    fn event(&mut self, name: &'static str, key: Option<&str>, kind: &str, payload: String) -> Vec<Op> {
        let group = match name {
            "swipe_card" | "swipe_back" | "swipe_fling" => Group::Scroll,
            _ => Group::ViewChange,
        };
        let event = Event {
            id: 0,
            key: key.map(str::to_string),
            kind: kind.to_string(),
            payload,
        };
        self.record(group, name, |runtime| runtime.dispatch(event))
    }
}

fn results() -> Value {
    let hours = json!([{"start": "08:00", "end": "23:00"}]);
    let cards: Vec<Value> = (0..CARDS)
        .map(|i| {
            json!({
                "place_id": format!("p{i}"),
                "name": format!("Restaurant number {i}"),
                "address": format!("{} Bank Street", 100 + i),
                "lat": 45.40 + i as f64 * 0.003,
                "lng": -75.69 + i as f64 * 0.002,
                "distance_m": 400 + i * 250,
                "price_tier": 1 + i % 4,
                "open_now": i % 3 != 0,
                "rating": 4.3,
                "user_rating_count": 812,
                "hours": {"mon": hours, "tue": hours, "wed": hours, "thu": hours, "fri": hours},
                "photos": ["/photos/a.jpg", "/photos/b.jpg", "/photos/c.jpg"],
                "amenities": ["dine_in", "wifi", "takeout"],
            })
        })
        .collect();
    json!({
        "cards": cards,
        "followups": ["Open now", "With patio", "Quieter", "Vegan"],
        "plan": {"time_context": {"local_datetime_iso": "2026-11-19T18:30:00-05:00"}},
    })
}

fn http_request(ops: &[Op]) -> u64 {
    ops.iter()
        .find_map(|op| match op {
            Op::Invoke {
                module,
                method,
                args,
            } if module == "http" && method == "request" && args.contains("/v1/search\"") => {
                serde_json::from_str::<Value>(args).ok()?["id"].as_u64()
            }
            _ => None,
        })
        .expect("the search sent its request")
}

/// Every flow, over RUNS fresh sessions.
pub fn run(viewport: Viewport) -> Vec<Flow> {
    let mut flows = Vec::new();
    let body = results().to_string();
    for _ in 0..RUNS {
        let start = Instant::now();
        let mut runtime = Runtime::new(NightshadeApp::remote("http://bench.local"));
        runtime.event(0, "host", HOST_INFO);
        let create_ms = elapsed_ms(start);
        let mut session = Session {
            runtime,
            flows,
            step: 0,
        };
        session.record(Group::Startup, "create_app", |_| Vec::new());
        session.record(Group::Startup, "first_frame", |runtime| runtime.render(viewport));
        // Creating the app (parsing its views) happened before recording.
        *session.flows[0].ms.last_mut().expect("just recorded") += create_ms;
        session.record(Group::ViewChange, "unchanged", |runtime| runtime.render(viewport));
        session.event("focus_search", Some("search"), "focus", String::new());
        session.event("type_letter", Some("search"), "change", "r".into());
        session.event("type_word", Some("search"), "change", "ramen open now".into());
        let ops = session.event("submit_search", Some("search"), "submit", String::new());
        let id = http_request(&ops);
        session.event("searching_tick", None, "timer", "searching".into());
        let response = json!({
            "module": "http", "method": "response", "id": id, "ok": true, "status": 200, "body": body,
        });
        session.event("show_results", None, "native", response.to_string());
        session.event("swipe_card", Some("places"), "page", "1".into());
        session.event("swipe_back", Some("places"), "page", "0".into());
        session.event("swipe_fling", Some("places"), "page", "9".into());
        session.event("tap_pin", Some("map"), "marker", "p4".into());
        let color = json!({"color": "#7a4b2c"}).to_string();
        session.event("photo_color", Some("photo:p4"), "load", color);
        session.event("save_place", Some("card:bookmark:p4"), "click", String::new());
        session.event("satellite", Some("map:type"), "click", String::new());
        let page = json!({"path": "/restaurant/p4", "transition": "zoom", "source": "card-p4"});
        session.event("open_page", None, "navigate", page.to_string());
        session.event("close_page", Some("page:1"), "dismiss", String::new());
        session.event("expand_sheet", Some("search-panel"), "select", "expanded".into());
        session.event("rest_sheet", Some("search-panel"), "select", "rest".into());
        let keyboard = HOST_INFO.replace("\"keyboardInset\": 0", "\"keyboardInset\": 336");
        assert_ne!(keyboard, HOST_INFO);
        session.event("keyboard_opens", None, "host", keyboard);
        let rotated = Viewport {
            width: viewport.height,
            height: viewport.width,
        };
        session.record(Group::ViewChange, "rotate", |runtime| runtime.render(rotated));
        session.record(Group::ViewChange, "rotate_back", |runtime| runtime.render(viewport));
        session.event("clear_search", Some("search:clear"), "click", String::new());
        flows = session.flows;
    }
    flows
}
