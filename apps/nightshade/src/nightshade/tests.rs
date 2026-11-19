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

    // The selected place's pin comes last, so it draws on top.
    let pins = app.pins();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[1]["id"], "p1");
    assert_eq!(pins[1]["state"], "selected");
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
    // Sized by its measured text, never a guessed width that cuts it off.
    assert_eq!(chip.props.width, None);
    assert_eq!(chip.props.align.as_deref(), Some("center"));
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
fn the_native_sheet_keeps_the_search_bar_above_the_keyboard() {
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

    // The layout ignores the keyboard: re-laying out on every keyboard frame
    // made the bar step. The native sheet moves it with the keyboard.
    host(&mut app, 34.0);
    assert_eq!(app.search_bottom_gap(), 4.0);
    host(&mut app, 335.0);
    assert_eq!(app.search_bottom_gap(), 4.0);

    let tree = render(&app);
    let raw = find(&tree, "search-panel").unwrap().props.native_props.clone().unwrap();
    let sheet: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(sheet["keyboardView"], "cheetah-search-bar");
}

#[test]
fn the_sheet_detents_follow_drags_both_ways() {
    let mut app = NightshadeApp::new(true);
    let sheet = |app: &NightshadeApp| -> serde_json::Value {
        let tree = render(app);
        let raw = find(&tree, "search-panel").unwrap().props.native_props.clone().unwrap();
        serde_json::from_str(&raw).expect("valid sheet JSON")
    };
    let drag_to = |app: &mut NightshadeApp, detent: &str| {
        app.update(Event {
            id: 0,
            key: Some("search-panel".into()),
            kind: "select".into(),
            payload: detent.into(),
        })
    };

    let at_rest = sheet(&app);
    assert_eq!(at_rest["selected"], "rest");
    assert_eq!(at_rest["detents"][0]["id"], "rest");
    assert_eq!(at_rest["detents"][0]["height"], 120.0);
    assert!(at_rest["detents"][1]["height"].as_f64().unwrap() > 500.0);

    drag_to(&mut app, "expanded");
    assert!(app.expanded);
    assert_eq!(sheet(&app)["selected"], "expanded");

    let mount = app.field_mount;
    drag_to(&mut app, "rest");
    assert!(!app.expanded);
    assert_eq!(app.field_mount, mount + 1, "the keyboard goes with it");
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
        let root = crate::tree::build(element);
        let frames = crate::layout::compute(&root, 393.0, 852.0, None);
        fn frame_of(
            node: &crate::tree::UiNode,
            frames: &std::collections::HashMap<u32, crate::layout::Frame>,
            key: &str,
        ) -> Option<crate::layout::Frame> {
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
        fn find_id(node: &crate::tree::UiNode) -> Option<u32> {
            if node.key.as_deref() == Some("search") {
                return Some(node.id);
            }
            node.children.iter().find_map(find_id)
        }
        find_id(&crate::tree::build(render(app))).expect("search field")
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

#[test]
fn result_pins_sit_between_the_status_bar_and_the_cards() {
    let (width, height) = (393.0, 852.0);
    let mut app = NightshadeApp::new(false);
    app.update(Event {
        id: 0,
        key: None,
        kind: "host".into(),
        payload: r#"{"safeTop":62,"safeBottom":34,"keyboardInset":34,"glass":true,"scale":3,"overlayChrome":true}"#.into(),
    });
    // "Best dessert spots near me" matches every sample place.
    click(&mut app, "suggest:1");
    assert!(app.visible_places().len() > 1, "a search with several places");

    let card_top = height - super::theme::CAROUSEL_BOTTOM - super::theme::CARD_HEIGHT;
    let check = |app: &NightshadeApp, only_selected: bool| {
        let (lat, lng, lat_delta, lng_delta) = app.region(width, height);
        for (index, place) in app.visible_places().iter().enumerate() {
            if only_selected && index != app.selected {
                continue;
            }
            let y = height / 2.0 - (place.lat - lat) / lat_delta * height;
            let x = width / 2.0 + (place.lng - lng) / lng_delta * width;
            assert!(y - 40.0 >= 62.0, "{} pin top {y} under the status bar", place.name);
            assert!(y <= card_top, "{} at {y} hidden by cards from {card_top}", place.name);
            assert!((0.0..=width).contains(&x), "{} at x {x} off screen", place.name);
        }
    };
    // Results open zoomed in on the first place.
    assert!(app.focused);
    assert_eq!(app.selected, 0);
    check(&app, true);

    // The overview of every pin (focus dropped) fits them all too.
    app.focused = false;
    check(&app, false);
    app.focused = true;
    // A focused card zooms to street level: under ~1 km of map on screen.
    let (_, _, focused_delta, _) = app.region(width, height);
    assert!(focused_delta < 0.009, "focused span {focused_delta} too wide");
}

#[test]
fn reopening_after_the_app_was_ended_resumes_its_state() {
    let file = std::env::temp_dir().join(format!("nightshade-test-{}.redb", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let lifecycle = |app: &mut NightshadeApp, payload: &str| {
        app.update(Event {
            id: 0,
            key: None,
            kind: "lifecycle".into(),
            payload: payload.into(),
        })
    };

    let mut app = NightshadeApp::new(false).with_state_file(&file);
    click(&mut app, "suggest:1");
    click(&mut app, "place:pizza");
    app.update(Event {
        id: 0,
        key: Some("search".into()),
        kind: "change".into(),
        payload: "late night ".into(),
    });
    let (selected, response) = (app.selected, app.response().cloned());
    assert!(response.is_some() && selected > 0);
    lifecycle(&mut app, "background");
    drop(app);

    let reopened = NightshadeApp::new(false).with_state_file(&file);
    assert_eq!(reopened.response().cloned(), response);
    assert_eq!(reopened.selected, selected);
    assert_eq!(reopened.query, "Best dessert spots near me");
    assert_eq!(reopened.draft, "late night ");
    assert!(!reopened.expanded, "reopens with the sheet down");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn search_time_gives_weekday_and_minute() {
    // 2026-11-17 is a Tuesday.
    assert_eq!(
        super::local_day_minute("2026-11-17T10:40:00.27-05:00"),
        Some((1, 640))
    );
    assert_eq!(super::local_day_minute(""), None);
}

#[test]
fn closed_cards_show_when_they_open() {
    let card = |hours: serde_json::Value| -> super::Place {
        serde_json::from_value::<super::ApiCard>(serde_json::json!({
            "place_id": "p", "name": "P", "lat": 45.0, "lng": -75.0,
            "open_now": false, "hours": hours,
        }))
        .unwrap()
        .into_place(Some((1, 640)))
    };
    let later = card(serde_json::json!({ "tue": [{ "start": "17:00", "end": "22:00" }] }));
    assert_eq!(later.opens, "5:00 PM");
    let tomorrow = card(serde_json::json!({ "wed": [{ "start": "09:00", "end": "21:00" }] }));
    assert_eq!(tomorrow.opens, "tomorrow at 9:00 AM");
    let friday = card(serde_json::json!({ "fri": [{ "start": "08:00", "end": "16:00" }] }));
    assert_eq!(friday.opens, "Fri at 8:00 AM");
    assert_eq!(card(serde_json::Value::Null).opens, "");
}

#[test]
fn google_rating_and_review_count_reach_the_card() {
    let place = serde_json::from_value::<super::ApiCard>(serde_json::json!({
        "place_id": "p", "name": "P", "lat": 45.0, "lng": -75.0,
        "rating": 4.36, "user_rating_count": 1234,
    }))
    .unwrap()
    .into_place(None);
    assert_eq!((place.rating.as_str(), place.reviews.as_str()), ("4.4", "1.2k"));
    assert_eq!(super::review_count(812), "812");
    assert_eq!(super::review_count(2000), "2k");
    assert_eq!(super::review_count(25_300), "25k");
}

#[test]
fn follow_ups_send_the_previous_plan() {
    let mut app = remote_app();
    submit(&mut app, "bakery");
    let id = request_id(&mut app);
    let body = serde_json::json!({"cards": [], "plan": {"categories": ["bakery"]}});
    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}),
    );

    submit(&mut app, "with seating");
    let calls = app.take_native_calls();
    let call = calls.iter().find(|call| call.module == "http" && call.method == "request").unwrap();
    let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
    let sent: serde_json::Value = serde_json::from_str(args["body"].as_str().unwrap()).unwrap();
    assert_eq!(sent["previous_plan"]["categories"][0], "bakery");

    // A fresh search after clearing starts over.
    click(&mut app, "search:clear");
    submit(&mut app, "sushi");
    let calls = app.take_native_calls();
    // The search, not the suggestions refresh that clearing also sends.
    let call = calls
        .iter()
        .find(|call| call.module == "http" && call.method == "request" && call.args.contains("\"POST\""))
        .unwrap();
    let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
    assert!(!args["body"].as_str().unwrap().contains("previous_plan"));
}

#[test]
fn searching_cycles_emojis_until_results_arrive() {
    use crate::app::Action;
    let timer = |app: &mut NightshadeApp| {
        app.update(Event {
            id: 0,
            key: None,
            kind: "timer".into(),
            payload: "searching".into(),
        })
    };

    let mut app = remote_app();
    submit(&mut app, "sushi");
    request_id(&mut app);
    assert!(matches!(app.take_actions().as_slice(), [Action::Timer { .. }]));
    assert_eq!(app.searching_frame, 0);

    timer(&mut app);
    assert_eq!(app.searching_frame, 1);
    assert_eq!(app.take_actions().len(), 1);

    // A second search while one timer is pending doesn't start another.
    submit(&mut app, "pizza");
    assert!(app.take_actions().is_empty());
    let id = request_id(&mut app);

    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": "{\"cards\":[]}"}),
    );
    timer(&mut app);
    assert!(app.take_actions().is_empty());
    assert!(!app.searching_timer);
}

#[test]
fn dragging_the_sheet_down_collapses_it() {
    let mut app = NightshadeApp::new(true);
    click(&mut app, "sheet:toggle");
    assert!(app.expanded);
    let mount = app.field_mount;

    app.update(Event {
        id: 0,
        key: Some("search-panel".into()),
        kind: "dismiss".into(),
        payload: String::new(),
    });
    assert!(!app.expanded);
    // The search field remounts, dropping the keyboard.
    assert_eq!(app.field_mount, mount + 1);
}

#[test]
fn the_map_button_switches_to_satellite_and_back() {
    let mut app = NightshadeApp::new(false);
    let map_type = |app: &NightshadeApp| {
        let map = find(&render(app), "map").expect("map").props.native_props.clone().unwrap();
        serde_json::from_str::<serde_json::Value>(&map).unwrap()["mapType"].clone()
    };
    assert_eq!(map_type(&app), "muted");
    assert!(find(&render(&app), "map:type").is_some(), "button shown");

    click(&mut app, "map:type");
    assert_eq!(map_type(&app), "hybrid");
    click(&mut app, "map:type");
    assert_eq!(map_type(&app), "muted");
}

#[test]
fn pins_are_map_children_at_their_coordinates() {
    let mut app = NightshadeApp::new(true);
    app.selected = 1;
    let tree = render(&app);
    let map = find(&tree, "map").expect("map");
    assert_eq!(map.children.len(), app.visible_places().len());
    for pin in &map.children {
        let key = pin.key.as_deref().unwrap();
        let place = app.visible_places().iter().find(|p| key == format!("pin:{}", p.id)).unwrap();
        assert_eq!(pin.props.map_latitude, Some(place.lat));
        assert_eq!(pin.props.map_longitude, Some(place.lng));
        // The marker fills the top half, so its tip sits on the coordinate.
        let marker = find(pin, &format!("place:{}", place.id)).expect("tappable marker");
        assert_eq!(marker.props.pressable, Some(true), "tapping a pin selects it");
        assert!((pin.props.height.unwrap() - 2.0 * marker.props.height.unwrap()).abs() < 0.01);
    }
    // The selected pin comes last (drawn on top) and is the larger marker.
    let last = map.children.last().unwrap();
    assert_eq!(last.key.as_deref(), Some(format!("pin:{}", app.visible_places()[1].id).as_str()));
    assert_eq!(last.props.width, Some(40.0));
    assert_eq!(map.children[0].props.width, Some(30.0));
}

#[test]
fn the_view_styles_the_native_sheet_and_carousel() {
    let app = NightshadeApp::new(true);
    let sheet_style = |app: &NightshadeApp| {
        let tree = render(app);
        let raw = find(&tree, "search-panel").expect("sheet").props.native_props.clone().expect("native_props");
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()
    };
    assert_eq!(sheet_style(&app)["background"], "#171717");
    assert_eq!(sheet_style(&app)["grabber"], false, "the view draws its own grabber");
    assert_eq!(sheet_style(&app)["cornerRadius"], 54);

    let tree = render(&app);
    assert_eq!(find(&tree, "places").expect("carousel").props.page_scale, Some(0.9));
}

#[test]
fn the_teardrop_marker_describes_a_valid_shape() {
    let app = NightshadeApp::new(true);
    let views = crate::views::Views::parse(&App::views(&app)).expect("views parse");
    let model = serde_json::json!({
        "pin": {"id": "p1", "lat": 45.4, "lng": -75.7},
        "w": 40, "h": 48, "outer": 96, "icon_size": 17, "look": views.constant("MARKER_RED"),
    });
    let pin = views.render("TeardropMarker", &model);
    let shape = &find(&pin, "place:p1").expect("marker").children[0];
    let look: serde_json::Value =
        serde_json::from_str(shape.props.native_props.as_deref().unwrap()).expect("valid JSON");
    assert!(look["path"].as_str().unwrap().ends_with('Z'));
    assert_eq!(look["viewBox"], serde_json::json!([100, 120]));
    assert_eq!(look["gradient"]["colors"][0], "#ff4a33");
}

#[test]
fn show_on_map_collapses_the_sheet_and_drops_the_keyboard() {
    let mut app = NightshadeApp::new(true);
    click(&mut app, "sheet:toggle");
    let mount = app.field_mount;
    click(&mut app, "show-on-map");
    assert!(!app.expanded);
    assert_eq!(app.field_mount, mount + 1);
}

#[test]
fn swiping_a_card_handle_minimizes_and_restores_the_cards() {
    let mut app = NightshadeApp::new(true);
    let swipe = |app: &mut NightshadeApp, direction: &str| {
        app.update(Event {
            id: 0,
            key: Some("handle:java".into()),
            kind: "gesture".into(),
            payload: format!(r#"{{"type":"swipe","state":"ended","direction":"{direction}"}}"#),
        })
    };
    let card = |app: &NightshadeApp| {
        let tree = render(app);
        find(&tree, "place-java").expect("card").props.height
    };
    assert_eq!(card(&app), Some(280.0));
    assert!(find(&render(&app), "handle:java").is_some(), "the card has a handle");

    swipe(&mut app, "down");
    assert!(app.cards_minimized);
    assert_eq!(card(&app), Some(72.0), "same card, shrunk");
    // The map gets the room the cards gave up.
    let (_, _, minimized_span, _) = app.region(393.0, 852.0);

    swipe(&mut app, "up");
    assert!(!app.cards_minimized);
    assert_eq!(card(&app), Some(280.0));
    let (_, _, full_span, _) = app.region(393.0, 852.0);
    assert!(minimized_span <= full_span);

    // A tap on the handle toggles; a tap on a small card restores.
    click(&mut app, "handle:java");
    assert!(app.cards_minimized);
    click(&mut app, "place-java");
    assert!(!app.cards_minimized);
}

#[test]
fn a_minimized_card_takes_its_photos_dominant_color() {
    let mut app = NightshadeApp::new(true);
    app.cards_minimized = true;
    let card = |app: &NightshadeApp| {
        let tree = render(app);
        find(&tree, "place-java").expect("card").props.bg.clone()
    };
    assert_eq!(card(&app).as_deref(), Some("#1f1f1f"), "before the photo loads");

    app.update(Event {
        id: 0,
        key: Some("photo:java".into()),
        kind: "load".into(),
        payload: r##"{"index":0,"color":"#2b3454"}"##.into(),
    });
    assert_eq!(card(&app).as_deref(), Some("#2b3454"));

    assert!(!super::components::is_light("#2b3454"), "navy takes white text");
    assert!(super::components::is_light("#f5e6c8"), "cream takes dark text");
}

#[test]
fn the_locate_button_centers_on_the_user_until_a_card_takes_over() {
    let mut app = NightshadeApp::new(true);
    let (width, height) = (393.0, 852.0);
    // No location yet: the button asks for one again.
    app.center = None;
    app.take_native_calls();
    click(&mut app, "map:locate");
    assert!(!app.locating);
    assert!(app.take_native_calls().iter().any(|call| call.module == "location"));

    app.center = Some((37.7700, -122.4150));
    let results = app.region(width, height);
    click(&mut app, "map:locate");
    assert!(app.locating);
    let located = app.region(width, height);
    assert_ne!(located, results, "the map moves to you");
    let tree = render(&app);
    let button = find(&tree, "map:locate").expect("locate button");
    assert!(format!("{:?}", button.children).contains("location.fill"), "filled while following");

    // Tapping again after panning by hand moves the map back: the region is
    // the same, so the map's `recenter` changes.
    let recenter = |app: &NightshadeApp| -> serde_json::Value {
        let tree = render(app);
        let raw = find(&tree, "map").unwrap().props.native_props.clone().unwrap();
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()["recenter"].clone()
    };
    let before = recenter(&app);
    click(&mut app, "map:locate");
    assert_eq!(app.region(width, height), located);
    assert_ne!(recenter(&app), before);

    // Picking a card hands the map back to the results.
    app.update(Event { id: 0, key: Some("places".into()), kind: "page".into(), payload: "1".into() });
    assert!(!app.locating);
}

#[test]
fn neighbouring_cards_peek_in_at_the_edges() {
    let mut app = NightshadeApp::new(true);
    app.selected = 1;
    let tree = render(&app);
    let carousel = find(&tree, "places").expect("carousel");
    let page = carousel.props.width.unwrap();
    let card = find(&tree, "place-java").unwrap().props.width.unwrap();
    // One card per page, narrower than the screen, centered, unclipped.
    assert!(page < 393.0 * 0.85 && card < page);
    assert_eq!(carousel.props.clip, Some(false));
    assert!((carousel.props.left.unwrap() - (393.0 - page) / 2.0).abs() < 0.01);
    // Paging moves one card at a time.
    assert!((carousel.props.scroll_x.unwrap() - page).abs() < 0.01);
}

#[test]
fn the_clear_button_is_glass_on_ios_26() {
    for glass in [false, true] {
        let mut app = NightshadeApp::new(true);
        app.host.glass = glass;
        let tree = render(&app);
        let clear = find(&tree, "search:clear").expect("clear button");
        assert_eq!(clear.kind == crate::element::Kind::Glass, glass);
    }
}

#[test]
fn show_on_map_fits_its_label() {
    let mut app = NightshadeApp::new(true);
    click(&mut app, "sheet:toggle");
    let tree = render(&app);
    let chip = find(&tree, "show-on-map").expect("chip in the expanded sheet");
    assert_eq!(chip.props.width, None, "sized by its label");
    let label = &chip.children[0];
    assert_eq!(label.props.width, None);
    assert_eq!(label.props.lines, Some(1), "never wraps out of the chip");

    // Laid out, it hugs its label rather than spanning the sheet.
    let mut runtime = crate::app::Runtime::new(app);
    let ops = runtime.render(Viewport { width: 393.0, height: 852.0 });
    let widths: Vec<f32> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Layout { width, height, .. } if (*height - 40.0).abs() < 0.01 => Some(*width),
            _ => None,
        })
        .collect();
    assert!(widths.iter().any(|w| *w < 200.0), "a 40pt-tall chip narrower than the sheet: {widths:?}");
}

#[test]
fn chips_are_glass_on_ios_26() {
    for glass in [false, true] {
        let mut app = NightshadeApp::new(true);
        app.host.glass = glass;
        click(&mut app, "sheet:toggle");
        let tree = render(&app);
        let chip = find(&tree, "followup:0").expect("follow-up chip");
        assert_eq!(chip.kind == crate::element::Kind::Glass, glass);
        let map_chip = find(&tree, "show-on-map").expect("show on map");
        assert_eq!(map_chip.kind == crate::element::Kind::Glass, glass);
        let label = if glass { &chip.children[0] } else { chip };
        assert_eq!(label.props.text.as_deref(), Some("Cafes within 12 min walk"));
        assert_eq!(label.props.width, None);
    }
}

#[test]
fn results_collapse_the_sheet_and_drop_the_keyboard() {
    let mut app = remote_app();
    app.update(Event { id: 0, key: Some("search".into()), kind: "focus".into(), payload: String::new() });
    assert!(app.expanded);
    submit(&mut app, "sushi");
    let id = request_id(&mut app);
    let mount = app.field_mount;
    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": "{\"cards\":[]}"}),
    );
    assert!(!app.expanded);
    // iOS won't shrink a sheet while the keyboard is up.
    assert_eq!(app.field_mount, mount + 1);
}

#[test]
fn a_backend_notice_replaces_the_results_line() {
    let mut app = remote_app();
    submit(&mut app, "halal tacos");
    let id = request_id(&mut app);
    let notice = "No taco spots nearby are labelled halal, so here are taco places. Check before you go.";
    let body = serde_json::json!({"cards": [], "notice": notice});
    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}),
    );
    assert_eq!(app.response().unwrap().notice.as_deref(), Some(notice));
    click(&mut app, "sheet:toggle");
    let tree = render(&app);
    assert!(format!("{tree:?}").contains("labelled halal"), "shown in the sheet");
}

#[test]
fn cards_page_through_the_places_photos() {
    let mut app = remote_app();
    submit(&mut app, "dairy queen");
    let id = request_id(&mut app);
    let body = serde_json::json!({"cards": [{
        "place_id": "dq", "name": "Dairy Queen", "lat": 45.4, "lng": -75.7, "distance_m": 10,
        "photos": ["/photos/dairy-queen.jpg", "https://example.com/menu.jpg"]
    }, {
        "place_id": "plain", "name": "Plain", "lat": 45.41, "lng": -75.71, "distance_m": 20
    }]});
    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}),
    );
    let tree = render(&app);
    let props = |id: &str| -> serde_json::Value {
        let photo = find(&tree, &format!("photo:{id}")).expect("photo");
        serde_json::from_str(photo.props.native_props.as_deref().unwrap()).unwrap()
    };
    let dq = props("dq");
    let photos = dq["photos"].as_array().unwrap();
    assert!(photos[0].as_str().unwrap().ends_with("/photos/dairy-queen.jpg"), "hero first");
    assert_eq!(photos[1], "https://example.com/menu.jpg");
    assert_eq!(props("plain")["pages"], 5, "no photos: the placeholder");
}

#[test]
fn tapping_a_card_opens_its_page_and_back_closes_it() {
    let mut runtime = Runtime::new(NightshadeApp::new(true));
    let viewport = Viewport { width: 393.0, height: 852.0 };
    let ops = runtime.render(viewport);
    // The card's node, by its test_id.
    let card = ops
        .iter()
        .find_map(|op| match op {
            Op::Create { id, props, .. } if props.test_id.as_deref() == Some("card-java") => Some(*id),
            _ => None,
        })
        .expect("java's card");
    let ops = runtime.event(card, "click", "");

    // The page opens: a native page that zooms out of the card.
    let page = ops.iter().find_map(|op| match op {
        Op::Create { props, .. } if props.native.as_deref() == Some("page") => props.native_props.clone(),
        _ => None,
    });
    let page: serde_json::Value = serde_json::from_str(&page.expect("page shown")).unwrap();
    assert_eq!(page["transition"], "zoom");
    assert_eq!(page["source"], "card-java");
    assert!(ops.iter().any(|op| matches!(op, Op::Create { props, .. } if props.text.as_deref() == Some("Java Haven"))));
    assert!(ops.iter().any(|op| matches!(op, Op::Invoke { module, method, .. } if module == "navigation" && method == "setCanGoBack")));

    // Dragged down by the user: back to the map.
    let page_id = ops
        .iter()
        .find_map(|op| match op {
            Op::Create { id, props, .. } if props.native.as_deref() == Some("page") => Some(*id),
            _ => None,
        })
        .unwrap();
    let ops = runtime.event(page_id, "dismiss", "");
    assert!(ops.iter().any(|op| matches!(op, Op::Remove { id } if *id == page_id)));
}

#[test]
fn a_page_closed_without_its_dismiss_does_not_block_the_next() {
    let mut app = NightshadeApp::new(true);
    click(&mut app, "card:up:java");
    let first = find(&render(&app), "page:1").is_some();
    assert!(first, "the first page");
    // iOS closed it but the app never heard; the user taps another card.
    click(&mut app, "card:up:pizza");
    let tree = render(&app);
    assert!(find(&tree, "page:2").is_some(), "a fresh native page");
    assert!(find(&tree, "page:1").is_none());
    assert_eq!(app.router.stack().len(), 2, "the stale page was closed, not stacked");
    assert_eq!(app.router.current().path, "/restaurant/pizza");
}

#[test]
fn a_restaurant_page_for_a_place_not_in_the_results_says_so() {
    let mut app = NightshadeApp::new(true);
    assert!(app.router.push("/restaurant/gone", "push", None));
    let tree = render(&app);
    assert!(format!("{tree:?}").contains("isn't in your results"));
}

#[test]
fn a_cards_up_arrow_opens_its_page() {
    let mut app = NightshadeApp::new(true);
    assert!(find(&render(&app), "card:up:java").is_some());
    click(&mut app, "card:up:java");
    let page = app.router.current();
    assert_eq!(page.path, "/restaurant/java");
    assert_eq!(page.transition, "zoom");
    assert_eq!(page.source.as_deref(), Some("card-java"), "grows out of the card, not the button");
}

#[test]
fn places_with_a_slash_in_their_id_open_too() {
    let mut app = remote_app();
    submit(&mut app, "shawarma");
    let id = request_id(&mut app);
    let body = serde_json::json!({"cards": [{
        "place_id": "osm:node/4269823295", "name": "Shawarma House", "lat": 45.4, "lng": -75.7, "distance_m": 10
    }]});
    http_response(
        &mut app,
        serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}),
    );
    click(&mut app, "card:up:osm:node/4269823295");
    assert_eq!(app.router.current().params["id"], "osm:node/4269823295");
    let tree = render(&app);
    assert!(format!("{tree:?}").contains("Shawarma House"), "the page found the place");
    assert!(!format!("{tree:?}").contains("isn't in your results"));
}

#[test]
fn satellite_and_minimized_cards_survive_a_relaunch() {
    let file = std::env::temp_dir().join(format!("nightshade-choices-{}.redb", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let background = Event { id: 0, key: None, kind: "lifecycle".into(), payload: "background".into() };

    let mut app = NightshadeApp::new(true).with_state_file(&file);
    click(&mut app, "map:type");
    click(&mut app, "handle:java");
    assert!(app.satellite && app.cards_minimized);
    app.update(background);

    let reopened = NightshadeApp::new(true).with_state_file(&file);
    assert!(reopened.satellite, "satellite mode is remembered");
    assert!(reopened.cards_minimized, "minimized cards are remembered");
    let _ = std::fs::remove_file(&file);
}

#[test]
fn card_and_page_buttons_share_save_and_route() {
    let mut app = NightshadeApp::new(true);
    app.take_native_calls();

    click(&mut app, "card:share:java");
    let calls = app.take_native_calls();
    let share = calls.iter().find(|call| call.module == "system" && call.method == "share").expect("share sheet");
    let args: serde_json::Value = serde_json::from_str(&share.args).unwrap();
    assert!(args["text"].as_str().unwrap().starts_with("Java Haven"));
    assert!(args["text"].as_str().unwrap().contains("maps.apple.com"));

    click(&mut app, "card:bookmark:java");
    assert!(app.is_saved("java"));
    let tree = render(&app);
    assert!(format!("{:?}", find(&tree, "card:bookmark:java").unwrap()).contains("bookmark.fill"));
    click(&mut app, "card:bookmark:java");
    assert!(!app.is_saved("java"), "a second tap un-saves");

    click(&mut app, "card:up:java");
    app.take_native_calls();
    click(&mut app, "page:directions");
    let calls = app.take_native_calls();
    let sheet = calls.iter().find(|call| call.module == "dialog" && call.method == "actionSheet").expect("asks");
    assert!(sheet.args.contains("Apple Maps") && sheet.args.contains("Google Maps"));

    let choose = |app: &mut NightshadeApp, index: i64| {
        app.update(Event {
            id: 0,
            key: None,
            kind: "native".into(),
            payload: format!(r#"{{"module":"dialog","method":"result","id":"directions:java","index":{index}}}"#),
        });
        app.take_native_calls()
    };
    let apple = choose(&mut app, 0);
    assert!(apple.iter().any(|call| call.method == "openUrl" && call.args.contains("maps.apple.com/?daddr=")));
    let google = choose(&mut app, 1);
    assert!(google.iter().any(|call| call.method == "openUrl" && call.args.contains("google.com/maps/dir/")));
    assert!(choose(&mut app, -1).iter().all(|call| call.method != "openUrl"), "cancel opens nothing");

    // The card's directions button asks the same way.
    click(&mut app, "card:directions:pizza");
    assert!(app.take_native_calls().iter().any(|call| call.method == "actionSheet"));
    click(&mut app, "page:bookmark");
    assert!(app.is_saved("java"));
}

#[test]
fn bookmarks_survive_a_relaunch() {
    let file = std::env::temp_dir().join(format!("nightshade-saved-{}.redb", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let mut app = NightshadeApp::new(true).with_state_file(&file);
    click(&mut app, "card:bookmark:pizza");
    app.update(Event { id: 0, key: None, kind: "lifecycle".into(), payload: "background".into() });
    assert!(NightshadeApp::new(true).with_state_file(&file).is_saved("pizza"));
    let _ = std::fs::remove_file(&file);
}

#[test]
fn suggestions_mix_your_searches_trending_and_the_time_of_day() {
    let mut app = remote_app();
    // Launch asked the backend; answer it.
    let mut app2 = NightshadeApp::with_backend(false, Backend::remote("http://api.test"));
    let calls = app2.take_native_calls();
    let call = calls.iter().find(|c| c.module == "http" && c.args.contains("/v1/search/suggestions")).expect("asks at launch");
    let id = serde_json::from_str::<serde_json::Value>(&call.args).unwrap()["id"].as_u64().unwrap();
    let body = serde_json::json!({"trending": ["Shawarma", "Bubble tea"], "time_of_day": ["Open now", "24 hour food"]});
    http_response(&mut app2, serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}));
    assert_eq!(app2.suggestions()[..4], ["Shawarma", "Bubble tea", "Open now", "24 hour food"]);

    // Your own searches come first.
    app2.remember_search("halal tacos");
    app2.remember_search("dairy queen");
    let chips = app2.suggestions();
    assert_eq!(chips[0], "dairy queen");
    assert_eq!(chips[1], "halal tacos");
    assert_eq!(chips.len(), 6);

    // Tapping a chip searches what it says.
    click(&mut app2, "suggest:2");
    assert_eq!(app2.query, "Shawarma");

    // Offline / no data: the fixed list.
    assert_eq!(app.suggestions().len(), 6);
    app.remember_search("  ");
    assert!(app.recent_searches.is_empty());
}

#[test]
fn a_place_open_only_through_its_drive_thru_says_so() {
    let card: super::ApiCard = serde_json::from_value(serde_json::json!({
        "place_id": "osm:way/1", "name": "McDonald's", "lat": 45.38, "lng": -75.64,
        "open_now": true, "closes_at_local": "00:00", "drive_thru_only": true
    }))
    .unwrap();
    let place = card.into_place(None);
    assert!(place.drive_thru_only);
    assert_eq!(place.open, Some(true));

    // Without the flag (older backends), it's just open.
    let card: super::ApiCard = serde_json::from_value(serde_json::json!({
        "place_id": "osm:way/2", "name": "Elgin Street Diner", "lat": 45.4, "lng": -75.69, "open_now": true
    }))
    .unwrap();
    assert!(!card.into_place(None).drive_thru_only);
}

#[test]
fn acting_on_a_result_reports_it_for_ranking() {
    let mut app = remote_app();
    submit(&mut app, "study spots");
    let calls = app.take_native_calls();
    let call = calls.iter().find(|c| c.module == "http" && c.args.contains("/v1/search\"")).unwrap();
    let id = serde_json::from_str::<serde_json::Value>(&call.args).unwrap()["id"].as_u64().unwrap();
    let body = serde_json::json!({
        "title": "Study", "plan": {"query_text": "study spots"}, "followups": [], "pins": [], "mode": "x",
        "warnings": [], "cards": [
            {"place_id": "a", "name": "A", "lat": 45.0, "lng": -75.0},
            {"place_id": "b", "name": "B", "lat": 45.01, "lng": -75.0}
        ]
    });
    http_response(&mut app, serde_json::json!({"id": id, "ok": true, "status": 200, "body": body.to_string()}));
    app.take_native_calls();

    click(&mut app, "card:bookmark:b");
    let calls = app.take_native_calls();
    let report = calls
        .iter()
        .find(|c| c.module == "http" && c.args.contains("/v1/search/feedback"))
        .expect("saving reports it");
    let args: serde_json::Value = serde_json::from_str(&report.args).unwrap();
    let sent: serde_json::Value = serde_json::from_str(args["body"].as_str().unwrap()).unwrap();
    assert_eq!(sent["query"], "study spots");
    assert_eq!(sent["place_id"], "b");
    assert_eq!(sent["action"], "save");
    assert_eq!(sent["position"], 1);

    // Unsaving says nothing.
    click(&mut app, "card:bookmark:b");
    assert!(!app.take_native_calls().iter().any(|c| c.args.contains("/v1/search/feedback")));
}
