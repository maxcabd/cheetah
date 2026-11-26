//! `cheetah-render`: a Cheetah app's screen as a PNG, on any machine. It runs
//! the app's own Rust and `.view` files and the runtime's layout, as a phone
//! would, then paints the result: no simulator, device or Mac needed. For
//! seeing a change at once, and for agents, which can't look at a phone.
//!
//!   cheetah-render [--app nightshade|nightshade-demo] [--out PATH]
//!                  [--width 393] [--height 852] [--scale 2] [--no-glass]
//!                  [--tap KEY] [--type KEY TEXT] [--event KIND KEY PAYLOAD]
//!                  [--tree]
//!
//! Taps, typing and events run in order, each on the screen the one before
//! left; pending timers then fire at once (animations end, cards build).
//! `--tree` also prints what's on screen, with keys to tap, as JSON. Native
//! views are close stand-ins: SF Symbols are simple line drawings, the map
//! is a plain grid, and web photos are gray.

// The app's source, compiled here as apps/nightshade compiles it (the
// runtime only includes it in device builds): the same paths through these.
mod app {
    pub use runtime::app::*;
}
mod element {
    pub use runtime::element::*;
}
mod gesture {
    pub use runtime::gesture::*;
}
mod http {
    pub use runtime::http::*;
}
mod kv {
    pub use runtime::kv::*;
}
mod native_api {
    pub use runtime::native_api::*;
}
mod resource {
    pub use runtime::resource::*;
}
mod router {
    pub use runtime::router::*;
}
mod updates {
    pub use runtime::updates::*;
}
#[cfg(test)]
mod diff {
    pub use runtime::diff::*;
}
#[cfg(test)]
mod views {
    pub use runtime::views::*;
}
#[cfg(test)]
mod tree {
    pub use runtime::tree::*;
}
#[cfg(test)]
mod layout {
    pub use runtime::layout::*;
}
#[allow(dead_code)]
#[path = "../../../apps/nightshade/src/nightshade/mod.rs"]
mod nightshade;

mod icons;
mod paint;
mod svg;
mod text;

use runtime::app::{Action, App, Event, ViewContext, Viewport};
use runtime::element::Element;
use runtime::views::Views;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// One thing to do to the app before rendering.
enum Step {
    Tap(String),
    Type(String, String),
    Event(String, String, String),
}

struct Options {
    app: String,
    out: Option<PathBuf>,
    width: f32,
    height: f32,
    scale: f32,
    glass: bool,
    tree: bool,
    assets: PathBuf,
    steps: Vec<Step>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        app: "nightshade".into(),
        out: None,
        width: 393.0,
        height: 852.0,
        scale: 2.0,
        glass: true,
        tree: false,
        assets: repo_root().join("ios/Cheetah/Assets"),
        steps: Vec::new(),
    };
    let value = |args: &mut dyn Iterator<Item = String>, flag: &str| {
        args.next().ok_or_else(|| format!("{flag} needs a value"))
    };
    let number = |text: String, flag: &str| text.parse::<f32>().map_err(|_| format!("{flag} needs a number"));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app" => options.app = value(&mut args, "--app")?,
            "--out" | "-o" => options.out = Some(value(&mut args, "--out")?.into()),
            "--width" => options.width = number(value(&mut args, "--width")?, "--width")?,
            "--height" => options.height = number(value(&mut args, "--height")?, "--height")?,
            "--scale" => options.scale = number(value(&mut args, "--scale")?, "--scale")?,
            "--no-glass" => options.glass = false,
            "--tree" => options.tree = true,
            "--assets" => options.assets = value(&mut args, "--assets")?.into(),
            "--tap" => options.steps.push(Step::Tap(value(&mut args, "--tap")?)),
            "--type" => {
                let key = value(&mut args, "--type")?;
                options.steps.push(Step::Type(key, value(&mut args, "--type")?));
            }
            "--event" => {
                let kind = value(&mut args, "--event")?;
                let key = value(&mut args, "--event")?;
                options.steps.push(Step::Event(kind, key, value(&mut args, "--event")?));
            }
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

fn make_app(name: &str) -> Result<nightshade::NightshadeApp, String> {
    match name {
        "nightshade" => Ok(nightshade::NightshadeApp::new(false)),
        "nightshade-demo" => Ok(nightshade::NightshadeApp::new(true)),
        other => Err(format!("unknown app {other}: nightshade or nightshade-demo")),
    }
}

fn event(kind: &str, key: Option<&str>, payload: &str) -> Event {
    Event { id: 0, key: key.map(str::to_string), kind: kind.into(), payload: payload.into() }
}

/// Fire the timers the app asked for, as if their time had passed, until
/// it asks for no more (bounded: a timer may ask again forever).
fn run_timers(app: &mut impl App) {
    for _ in 0..200 {
        let timers: Vec<String> = app
            .take_actions()
            .into_iter()
            .filter_map(|action| match action {
                Action::Timer { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        if timers.is_empty() {
            return;
        }
        for name in timers {
            app.update(event("timer", None, &name));
        }
    }
}

fn view(app: &impl App, views: &Views, viewport: Viewport, measurer: &text::Measurer) -> Element {
    app.view(&ViewContext::new(viewport, Some(measurer)).with_views(views))
}

fn find<'e>(element: &'e Element, key: &str) -> Option<&'e Element> {
    if element.key.as_deref() == Some(key) {
        return Some(element);
    }
    element.children.iter().find_map(|child| find(child, key))
}

fn run(options: Options) -> Result<serde_json::Value, String> {
    let fonts = text::Fonts::load(&options.assets)?;
    let measurer = text::Measurer { fonts: &fonts };
    let viewport = Viewport { width: options.width, height: options.height };
    let mut app = make_app(&options.app)?;
    let views = Views::parse(&app.views()).map_err(|error| format!("views: {error:?}"))?;
    // What an iPhone reports: notch and home indicator, Liquid Glass.
    app.update(event(
        "host",
        None,
        &json!({"safeTop": 59, "safeBottom": 34, "glass": options.glass, "scale": options.scale}).to_string(),
    ));
    run_timers(&mut app);

    for step in &options.steps {
        match step {
            Step::Tap(key) => {
                let screen = view(&app, &views, viewport, &measurer);
                let element = find(&screen, key).ok_or_else(|| format!("nothing to tap with key {key}"))?;
                // A `link` navigates, as the host reports it; the rest click.
                match element.props.link.as_deref() {
                    Some(path) => {
                        let payload = json!({
                            "path": path,
                            "transition": element.props.transition.as_deref().unwrap_or("push"),
                            "source": element.props.test_id,
                        });
                        app.update(event("navigate", Some(key), &payload.to_string()));
                    }
                    None => app.update(event("click", Some(key), "")),
                }
            }
            Step::Type(key, typed) => app.update(event("change", Some(key), typed)),
            Step::Event(kind, key, payload) => app.update(event(kind, Some(key), payload)),
        }
        // Native calls (haptics, location, http) have no host here.
        let _ = app.take_native_calls();
        run_timers(&mut app);
    }

    let screen = view(&app, &views, viewport, &measurer);
    let root = runtime::tree::build(screen);
    let frames = runtime::layout::compute(&root, options.width, options.height, Some(&measurer));
    let (width, height) = ((options.width * options.scale).round() as u32, (options.height * options.scale).round() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("bad size")?;
    pixmap.fill(paint::color("#000000", 1.0));
    let mut painter = paint::Painter::new(&fonts, &frames, options.assets.clone(), options.scale);
    painter.paint(&mut pixmap, &root);

    let out = options.out.clone().unwrap_or_else(|| repo_root().join("target/render").join(format!("{}.png", options.app)));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    pixmap.save_png(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut report = json!({"ok": true, "png": out.display().to_string(), "size": [width, height]});
    if options.tree {
        report["elements"] = painter.drawn.into();
    }
    Ok(report)
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("cheetah-render: {error}");
            }
            eprintln!("{}", include_str!("main.rs").lines().take_while(|line| line.starts_with("//!")).map(|line| line.trim_start_matches("//!").strip_prefix(' ').unwrap_or("")).collect::<Vec<_>>().join("\n"));
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!({"ok": false, "error": error}));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    fn options(app: &str, steps: Vec<Step>, out: &str) -> Options {
        let mut options = parse(["--app", app, "--tree", "--out", out].into_iter().map(String::from)).unwrap();
        options.steps = steps;
        options
    }

    fn keys(report: &serde_json::Value) -> Vec<String> {
        report["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|element| element["key"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn renders_a_screen_and_says_what_is_on_it() {
        let out = std::env::temp_dir().join(format!("cheetah-render-{}.png", std::process::id()));
        let report = run(options("nightshade", vec![], out.to_str().unwrap())).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["size"], json!([786, 1704]));
        let image = image::open(&out).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (786, 1704));
        // Drawn, not blank: the map, the sheet, text.
        let colors: std::collections::HashSet<_> = image.pixels().map(|pixel| pixel.0).collect();
        assert!(colors.len() > 500, "only {} colors", colors.len());
        let keys = keys(&report);
        assert!(keys.contains(&"suggest:0".to_string()) && keys.contains(&"drawer:open".to_string()));
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn taps_run_before_the_render() {
        let out = std::env::temp_dir().join(format!("cheetah-render-tap-{}.png", std::process::id()));
        let report = run(options(
            "nightshade-demo",
            vec![Step::Tap("place-java".into())],
            out.to_str().unwrap(),
        ))
        .unwrap();
        assert!(keys(&report).contains(&"page:bookmark".to_string()), "the card opened its page");
        let error = run(options("nightshade", vec![Step::Tap("nope".into())], out.to_str().unwrap())).unwrap_err();
        assert!(error.contains("nope"));
        let _ = std::fs::remove_file(out);
    }
}
