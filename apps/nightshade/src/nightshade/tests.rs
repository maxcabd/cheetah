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
fn search_sends_location_only_inside_the_market() {
    let glebe = super::search_body("sushi", Some((45.4005, -75.6870)));
    assert_eq!(glebe["text"], "sushi");
    assert!((glebe["lat"].as_f64().unwrap() - 45.4005).abs() < 1e-4);
    assert!((glebe["lng"].as_f64().unwrap() + 75.6870).abs() < 1e-4);

    // Emulator default (Mountain View) and no fix: backend default center.
    let far = super::search_body("sushi", Some((37.422, -122.084)));
    assert!(far.get("lat").is_none() && far.get("lng").is_none());
    assert!(super::search_body("sushi", None).get("lat").is_none());

    let ottawa_to_toronto = super::distance_km((45.4215, -75.6972), (43.6532, -79.3832));
    assert!((ottawa_to_toronto - 352.0).abs() < 5.0);
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
        payload:
            r#"{"module":"location","method":"result","ok":true,"latitude":40.0,"longitude":-73.0}"#
                .into(),
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

fn find<'a>(
    element: &'a crate::element::Element,
    key: &str,
) -> Option<&'a crate::element::Element> {
    if element.key.as_deref() == Some(key) {
        return Some(element);
    }
    element.children.iter().find_map(|child| find(child, key))
}

fn render(app: &NightshadeApp) -> crate::element::Element {
    // A broken .view file must fail here, not when the app starts.
    let views = crate::views::Views::parse(&App::views(app)).expect("views parse");
    let context = crate::app::ViewContext::new(
        Viewport {
            width: 393.0,
            height: 852.0,
        },
        None,
    )
    .with_views(&views);
    app.view(&context)
}

#[test]
fn views_render_the_empty_sheet() {
    let app = NightshadeApp::new(false);
    let screen = render(&app);
    assert!(find(&screen, "map").is_some());
    assert!(find(&screen, "places").is_none());
    let chip = find(&screen, "suggest:5").expect("last suggestion chip");
    assert_eq!(
        chip.props.text.as_deref(),
        Some("Nearby spots that open late night")
    );
    assert_eq!(
        chip.props.width,
        Some(NightshadeApp::chip_width(
            "Nearby spots that open late night"
        ))
    );
}

#[test]
fn views_render_result_cards() {
    let mut app = NightshadeApp::new(true);
    let screen = render(&app);
    let places = find(&screen, "places").expect("carousel");
    assert_eq!(places.children.len(), app.visible_places().len());
    assert!(find(&screen, "search:clear").is_some());
    assert!(find(&screen, "followup:0").is_none());

    click(&mut app, "sheet:toggle");
    let screen = render(&app);
    assert!(find(&screen, "followup:1").is_some());
}

#[test]
fn runtime_version_matches_cheetah_toml() {
    // Over-the-air bundles are published for `version+build`.
    let config = include_str!("../../cheetah.toml");
    let field = |name: &str| {
        config
            .lines()
            .find_map(|line| line.strip_prefix(name)?.trim_start().strip_prefix('='))
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap()
    };
    assert_eq!(
        super::RUNTIME_VERSION,
        format!("{}+{}", field("version"), field("build"))
    );
}

#[test]
fn search_field_expands_and_swaps_voice_for_send() {
    let mut app = NightshadeApp::new(false);
    let search = |app: &mut NightshadeApp, kind: &str, payload: &str| {
        app.update(Event {
            id: 0,
            key: Some("search".into()),
            kind: kind.into(),
            payload: payload.into(),
        })
    };

    let screen = render(&app);
    let field = find(&screen, "search").expect("search field");
    assert_eq!(
        field.props.placeholder.as_deref(),
        Some("What are you in the mood for?")
    );
    assert!(find(&screen, "search:voice").is_some());
    assert!(find(&screen, "search:send").is_none());

    search(&mut app, "focus", "");
    assert!(app.expanded);

    search(&mut app, "change", "tacos");
    let screen = render(&app);
    assert!(find(&screen, "search:voice").is_none());
    assert!(find(&screen, "search:send").is_some());
}

#[test]
fn open_keyboard_lifts_the_search_bar_above_it() {
    let mut app = NightshadeApp::new(false);
    let host = |app: &mut NightshadeApp, keyboard: f32| {
        app.update(Event {
            id: 0,
            key: None,
            kind: "host".into(),
            payload: format!(
                r#"{{"safeTop":62,"safeBottom":34,"keyboardInset":{keyboard},"glass":true,"scale":3,"overlayChrome":true}}"#
            ),
        })
    };

    // At rest the inset is the safe area, not a keyboard.
    host(&mut app, 34.0);
    assert_eq!(app.search_bottom_gap(), 4.0);

    host(&mut app, 335.0);
    assert_eq!(app.search_bottom_gap(), 343.0);
}

#[test]
fn search_field_fills_the_bar_with_and_without_glass() {
    for glass in [false, true] {
        let mut app = NightshadeApp::new(false);
        app.update(Event {
            id: 0,
            key: None,
            kind: "host".into(),
            payload: format!(r#"{{"safeTop":62,"safeBottom":34,"keyboardInset":34,"glass":{glass},"scale":3,"overlayChrome":true}}"#),
        });
        let element = render(&app);
        let root = runtime::build(element);
        let frames = runtime::layout::compute(&root, 393.0, 852.0, None);
        fn frame_of(
            node: &runtime::UiNode,
            frames: &std::collections::HashMap<u32, runtime::Frame>,
            key: &str,
        ) -> Option<runtime::Frame> {
            if node.key.as_deref() == Some(key) {
                return frames.get(&node.id).copied();
            }
            node.children
                .iter()
                .find_map(|child| frame_of(child, frames, key))
        }
        let field = frame_of(&root, &frames, "search").expect("search field");
        let voice = frame_of(&root, &frames, "search:voice").expect("voice icon");
        // The bar is 345 wide (393 minus 24 on each side); the icon ends
        // 8 short of it, inside the pill's rounded end.
        assert_eq!(field.x, 0.0, "glass={glass}");
        assert_eq!(voice.x + voice.width, 337.0, "glass={glass}");
        assert_eq!(field.width, voice.x, "glass={glass}");
    }
}

#[test]
fn sending_remounts_the_field_to_dismiss_the_keyboard() {
    fn field_id(app: &NightshadeApp) -> u32 {
        fn find_id(node: &runtime::UiNode) -> Option<u32> {
            if node.key.as_deref() == Some("search") {
                return Some(node.id);
            }
            node.children.iter().find_map(find_id)
        }
        find_id(&runtime::build(render(app))).expect("search field")
    }
    let mut app = NightshadeApp::new(false);
    let search = |app: &mut NightshadeApp, kind: &str, payload: &str| {
        app.update(Event {
            id: 0,
            key: Some("search".into()),
            kind: kind.into(),
            payload: payload.into(),
        })
    };

    search(&mut app, "change", "coffee");
    let typing = field_id(&app);
    search(&mut app, "change", "coffee shops");
    assert_eq!(field_id(&app), typing, "typing keeps the field");

    search(&mut app, "submit", "");
    let after_return = field_id(&app);
    assert_ne!(after_return, typing, "return remounts the field");

    search(&mut app, "change", "tea");
    click(&mut app, "search:send");
    assert_ne!(field_id(&app), after_return, "the send arrow remounts it too");
}
