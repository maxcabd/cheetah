use crate::diff::{diff, Op};
use crate::element::{Element, Props};
use crate::layout::{compute, compute_batched, Frame, TextMeasurer};
use crate::native_api::NativeCall;
use crate::tree::{build, flatten, FlatTree};
use serde::Deserialize;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HostInfo {
    pub safe_top: f32,
    pub safe_bottom: f32,
    pub keyboard_inset: f32,
    pub overlay_chrome: bool,
    pub glass: bool,
    pub scale: f32,
}

impl Default for HostInfo {
    fn default() -> Self {
        Self {
            safe_top: 0.0,
            safe_bottom: 0.0,
            keyboard_inset: 0.0,
            overlay_chrome: false,
            glass: false,
            scale: 1.0,
        }
    }
}

pub struct Event {
    pub id: u32,
    pub key: Option<String>,
    pub kind: String,
    pub payload: String,
}

pub fn event_kind_from_code(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "click",
        2 => "change",
        3 => "submit",
        4 => "scroll",
        5 => "page",
        6 => "focus",
        7 => "blur",
        8 => "marker",
        9 => "host",
        10 => "incoming",
        11 => "ack",
        12 => "fail",
        13 => "flush",
        14 => "connection",
        15 => "select",
        16 => "native",
        17 => "gesture",
        18 => "refresh",
        19 => "load",
        20 => "dismiss",
        _ => return None,
    })
}

pub enum Action {
    SendMessage { id: u32, body: String },
    /// After `ms`, deliver a `timer` event with `name` as its payload. For
    /// animations driven from `update`, such as a loading indicator.
    Timer { name: String, ms: u64 },
}

pub struct ViewContext<'a> {
    pub viewport: Viewport,
    measurer: Option<&'a dyn TextMeasurer>,
    views: Option<&'a crate::views::Views>,
}

impl<'a> ViewContext<'a> {
    pub fn new(viewport: Viewport, measurer: Option<&'a dyn TextMeasurer>) -> Self {
        Self {
            viewport,
            measurer,
            views: None,
        }
    }

    /// Attach parsed `.view` templates (the runtime does this for `view`).
    pub fn with_views(mut self, views: &'a crate::views::Views) -> Self {
        self.views = Some(views);
        self
    }

    /// Render the `.view` component `name` with `model` (any `Serialize`
    /// value; its fields are what the template's paths read).
    pub fn view(&self, name: &str, model: &impl serde::Serialize) -> Element {
        let model = serde_json::to_value(model).unwrap_or(serde_json::Value::Null);
        match self.views {
            Some(views) => views.render(name, &model),
            None => crate::views::Views::default().render(name, &model),
        }
    }

    pub fn measure_text(&self, text: &str, size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        match self.measurer {
            Some(measurer) => measurer.measure(text, size, bold, max_width),
            None => crate::layout::rough_measure(text, size, max_width),
        }
    }
}

pub trait App {
    fn view(&self, context: &ViewContext) -> Element;
    fn update(&mut self, event: Event);
    fn take_actions(&mut self) -> Vec<Action> {
        Vec::new()
    }
    fn has_queued(&self) -> bool {
        false
    }
    /// Queue imperative platform calls for the next rendered frame.
    fn take_native_calls(&mut self) -> Vec<NativeCall> {
        Vec::new()
    }
    /// Replacement `.view` files, checked before every render; for
    /// over-the-air updates (see `runtime::updates`).
    fn take_views(&mut self) -> Option<Vec<(String, String)>> {
        None
    }
    /// The app's `.view` files as `(path, source)`, usually embedded with
    /// `include_str!`. Parsed once at startup; the dev server can replace
    /// them in a running app.
    fn views(&self) -> Vec<(&'static str, &'static str)> {
        Vec::new()
    }
    /// The app's code was hot-patched (dev server). Drop anything derived
    /// from the old code, e.g. memoized elements, before the re-render.
    fn hot_reloaded(&mut self) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeMetrics {
    /// The app's `update` for the events since the last render, and how many.
    pub update_us: u64,
    pub events: usize,
    pub build_us: u64,
    pub diff_us: u64,
    pub prime_us: u64,
    pub layout_us: u64,
    pub nodes: usize,
    pub ops: usize,
    pub text_requests: usize,
}

pub struct Runtime {
    app: Box<dyn App>,
    previous: Option<FlatTree>,
    frames: HashMap<u32, Frame>,
    viewport: Viewport,
    text_measurer: Option<Rc<dyn TextMeasurer>>,
    layout_dirty: bool,
    metrics: RuntimeMetrics,
    /// Since the last render: time in `update`, events, and the first one's
    /// kind and key family ("click place"), which the frame is put down to.
    update_us: u64,
    events: usize,
    trigger: Option<String>,
    /// Rendered since `take_frame`: events before the first resize don't.
    rendered: bool,
    /// Literal values edited live by the dev server, applied after `view`.
    #[cfg(feature = "devserver")]
    literals: crate::live::LiteralOverrides,
    views: crate::views::Views,
}

impl Runtime {
    pub fn new<A: App + 'static>(app: A) -> Self {
        // A broken embedded view is a build mistake: fail loudly with its
        // file and line (the app's tests parse its views too).
        let views = crate::views::Views::parse(&app.views())
            .unwrap_or_else(|error| panic!("invalid .view file: {error}"));
        Self {
            app: Box::new(app),
            previous: None,
            frames: HashMap::new(),
            viewport: Viewport::default(),
            text_measurer: None,
            layout_dirty: true,
            metrics: RuntimeMetrics::default(),
            update_us: 0,
            events: 0,
            trigger: None,
            rendered: false,
            #[cfg(feature = "devserver")]
            literals: Default::default(),
            views,
        }
    }

    pub fn metrics(&self) -> RuntimeMetrics {
        self.metrics
    }

    /// The frame rendered since the last call: its metrics, and what caused
    /// it if an event did. None when nothing was rendered.
    pub fn take_frame(&mut self) -> Option<(RuntimeMetrics, Option<String>)> {
        std::mem::take(&mut self.rendered).then(|| (self.metrics, self.trigger.take()))
    }

    fn update(&mut self, event: Event) {
        if self.trigger.is_none() {
            self.trigger = Some(trigger(&event));
        }
        let start = Instant::now();
        self.app.update(event);
        self.update_us += start.elapsed().as_micros() as u64;
        self.events += 1;
    }

    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.text_measurer = Some(measurer);
        self.layout_dirty = true;
    }

    /// Re-render at the current viewport after the app's code changed.
    pub fn rerender(&mut self) -> Vec<Op> {
        self.app.hot_reloaded();
        self.refresh()
    }

    /// Render again at the current viewport, forcing layout.
    pub fn refresh(&mut self) -> Vec<Op> {
        self.layout_dirty = true;
        let viewport = self.viewport;
        self.render(viewport)
    }

    /// Replace the app's `.view` templates (dev server) and re-render.
    #[cfg(feature = "devserver")]
    pub fn set_views(&mut self, views: crate::views::Views) -> Vec<Op> {
        self.views = views;
        self.rerender()
    }

    /// Replace the live literal overrides (dev server) and re-render.
    #[cfg(feature = "devserver")]
    pub fn set_literals(&mut self, literals: crate::live::LiteralOverrides) -> Vec<Op> {
        self.literals = literals;
        self.refresh()
    }

    pub fn render(&mut self, viewport: Viewport) -> Vec<Op> {
        // New `.view` templates from the app (over-the-air updates). Invalid
        // ones are ignored: the views on screen stay.
        if let Some(files) = self.app.take_views() {
            let sources: Vec<(&str, &str)> = files
                .iter()
                .map(|(name, source)| (name.as_str(), source.as_str()))
                .collect();
            if let Ok(views) = crate::views::Views::parse(&sources) {
                self.views = views;
                self.app.hot_reloaded();
            }
        }
        self.rendered = true;
        self.metrics.update_us = std::mem::take(&mut self.update_us);
        self.metrics.events = std::mem::take(&mut self.events);
        let build_start = Instant::now();
        let root = {
            let context =
                ViewContext::new(viewport, self.text_measurer.as_deref()).with_views(&self.views);
            #[allow(unused_mut)]
            let mut root = self.app.view(&context);
            #[cfg(feature = "devserver")]
            if !self.literals.is_empty() {
                crate::live::apply(&mut root, &self.literals);
            }
            build(root)
        };
        let flat = flatten(&root);
        self.metrics.build_us = build_start.elapsed().as_micros() as u64;
        self.metrics.nodes = flat.len();

        // Scrolling and typing still rebuild the view tree, but an unchanged
        // tree with an unchanged viewport must not pay for Taffy again.
        if !self.layout_dirty && viewport == self.viewport && self.previous.as_ref() == Some(&flat)
        {
            self.metrics.diff_us = 0;
            self.metrics.prime_us = 0;
            self.metrics.layout_us = 0;
            self.metrics.text_requests = 0;
            let mut ops = Vec::new();
            self.append_native_calls(&mut ops);
            self.metrics.ops = ops.len();
            return ops;
        }

        let diff_start = Instant::now();
        let mut ops = diff(self.previous.as_ref(), &flat);
        self.metrics.diff_us = diff_start.elapsed().as_micros() as u64;
        let layout_needed = self.layout_dirty
            || viewport != self.viewport
            || self
                .previous
                .as_ref()
                .is_none_or(|previous| layout_required(previous, &flat));

        self.metrics.prime_us = 0;
        self.metrics.layout_us = 0;
        self.metrics.text_requests = 0;
        if layout_needed {
            self.viewport = viewport;
            self.layout_dirty = false;
            let layout_start = Instant::now();
            let (frames, measured) = match self.text_measurer.as_deref() {
                Some(measurer) => {
                    compute_batched(&root, viewport.width, viewport.height, measurer)
                }
                None => (
                    compute(&root, viewport.width, viewport.height, None),
                    Default::default(),
                ),
            };
            self.metrics.text_requests = measured.texts;
            self.metrics.prime_us = measured.time.as_micros() as u64;
            self.metrics.layout_us = layout_start
                .elapsed()
                .saturating_sub(measured.time)
                .as_micros() as u64;

            for (id, frame) in &frames {
                let changed = self
                    .frames
                    .get(id)
                    .map(|previous| previous != frame)
                    .unwrap_or(true);
                if changed {
                    ops.push(Op::Layout {
                        id: *id,
                        x: frame.x,
                        y: frame.y,
                        width: frame.width,
                        height: frame.height,
                    });
                }
            }

            self.frames = frames;
        }

        self.previous = Some(flat);
        self.append_native_calls(&mut ops);
        self.metrics.ops = ops.len();
        ops
    }

    fn append_native_calls(&mut self, ops: &mut Vec<Op>) {
        for call in self.app.take_native_calls() {
            ops.push(Op::Invoke {
                module: call.module,
                method: call.method,
                args: call.args,
            });
        }
    }

    pub fn apply_event(&mut self, id: u32, kind: &str, payload: &str) {
        let node = self.previous.as_ref().and_then(|tree| tree.get(&id));
        let key = node.and_then(|node| node.key.clone());

        // A pressed `link` navigates: the app gets `navigate` with the path,
        // how to open it, and the element to grow from (its test_id).
        if kind == "click" {
            if let Some(path) = node.and_then(|node| node.props.link.clone()) {
                let props = &node.expect("a link comes from a node").props;
                self.update(Event {
                    id,
                    key,
                    kind: "navigate".into(),
                    payload: serde_json::json!({
                        "path": path,
                        "transition": props.transition.as_deref().unwrap_or("push"),
                        "source": props.test_id,
                    })
                    .to_string(),
                });
                return;
            }
        }

        self.update(Event {
            id,
            key,
            kind: kind.to_string(),
            payload: payload.to_string(),
        });
    }

    pub fn render_current(&mut self) -> Vec<Op> {
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Vec::new();
        }
        self.render(self.viewport)
    }

    pub fn event(&mut self, id: u32, kind: &str, payload: &str) -> Vec<Op> {
        self.apply_event(id, kind, payload);
        self.render_current()
    }

    pub fn dispatch(&mut self, event: Event) -> Vec<Op> {
        self.update(event);
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Vec::new();
        }
        self.render(self.viewport)
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        self.app.take_actions()
    }

    pub fn has_queued(&self) -> bool {
        self.app.has_queued()
    }
}

/// "click place" for a click on `place:java`: the kind, and the key up to
/// its id, so frames group by what was done and not by which item.
fn trigger(event: &Event) -> String {
    let family = match &event.key {
        Some(key) => key.split([':', '-']).next().unwrap_or(""),
        None if event.kind == "timer" => event.payload.as_str(),
        None => "",
    };
    if family.is_empty() {
        event.kind.clone()
    } else {
        format!("{} {family}", event.kind)
    }
}

fn layout_required(previous: &FlatTree, next: &FlatTree) -> bool {
    if previous.len() != next.len() {
        return true;
    }

    for (id, node) in next {
        let Some(old) = previous.get(id) else {
            return true;
        };
        if old.kind != node.kind
            || old.parent != node.parent
            || old.children != node.children
            || props_affect_layout(&old.props, &node.props)
        {
            return true;
        }
    }

    false
}

fn props_affect_layout(previous: &Props, next: &Props) -> bool {
    if previous.width != next.width
        || previous.height != next.height
        || previous.pad != next.pad
        || previous.pad_h != next.pad_h
        || previous.pad_v != next.pad_v
        || previous.pad_bottom != next.pad_bottom
        || previous.content_width != next.content_width
        || previous.weight != next.weight
        || previous.gap != next.gap
        || previous.align_items != next.align_items
        || previous.justify != next.justify
        || previous.absolute != next.absolute
        || previous.top != next.top
        || previous.left != next.left
        || previous.bottom != next.bottom
        || previous.right != next.right
        || previous.size != next.size
        || previous.bold != next.bold
        || previous.font != next.font
        || previous.line_height != next.line_height
        || previous.lines != next.lines
        || previous.wrap != next.wrap
        || previous.placeholder != next.placeholder
    {
        return true;
    }

    previous.text != next.text && !(has_fixed_size(previous) && has_fixed_size(next))
}

fn has_fixed_size(props: &Props) -> bool {
    props.width.is_some() && props.height.is_some()
}

#[cfg(test)]
mod tests {
    use super::{App, Event, HostInfo, Runtime, Viewport};
    use crate::diff::Op;
    use crate::element::{button, column, text, Element};
    use crate::layout::TextMeasurer;
    use crate::native_api::NativeCall;
    use std::cell::Cell;
    use std::rc::Rc;

    struct CountingMeasurer {
        calls: Cell<usize>,
    }

    impl TextMeasurer for CountingMeasurer {
        fn measure(&self, _: &str, _: f32, _: bool, max_width: f32) -> (f32, f32) {
            self.calls.set(self.calls.get() + 1);
            (max_width.min(50.0), 20.0)
        }
    }

    struct StaticApp;

    impl App for StaticApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            column().width(300.0).height(600.0).child(text("hello"))
        }

        fn update(&mut self, _: Event) {}
    }

    #[test]
    fn frames_are_put_down_to_what_caused_them() {
        let mut runtime = Runtime::new(StaticApp);
        // No size yet: the event is applied, but nothing is rendered.
        runtime.event(0, "host", "{}");
        assert!(runtime.take_frame().is_none());

        runtime.render(Viewport {
            width: 300.0,
            height: 600.0,
        });
        let (metrics, trigger) = runtime.take_frame().expect("rendered");
        assert_eq!(trigger.as_deref(), Some("host"));
        assert_eq!((metrics.events, metrics.nodes), (1, 2));
        assert!(runtime.take_frame().is_none());

        for (key, kind, payload, name) in [
            (Some("place:java"), "click", "", "click place"),
            (Some("place-java"), "click", "", "click place"),
            (None, "timer", "searching", "timer searching"),
            (None, "native", "{}", "native"),
        ] {
            runtime.dispatch(Event {
                id: 0,
                key: key.map(str::to_string),
                kind: kind.into(),
                payload: payload.into(),
            });
            let (_, trigger) = runtime.take_frame().expect("rendered");
            assert_eq!(trigger.as_deref(), Some(name));
        }
    }

    /// Wraps at `max_width`, 10 wide a letter, and counts its host calls.
    struct BatchHost {
        batches: std::sync::Mutex<Vec<usize>>,
        singles: std::sync::atomic::AtomicUsize,
    }

    fn wrapped(text: &str, max_width: f32) -> (f32, f32) {
        let max_width = max_width.max(10.0);
        let full = text.len() as f32 * 10.0;
        (full.min(max_width), 20.0 * (full / max_width).ceil().max(1.0))
    }

    impl crate::bridge::Host for BatchHost {
        fn measure_text(&self, text: &str, _: f32, _: bool, max_width: f32) -> (f32, f32) {
            self.singles
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            wrapped(text, max_width)
        }

        fn measure_text_batch(&self, requests: &[crate::layout::TextRequest]) -> Vec<(f32, f32)> {
            self.batches.lock().unwrap().push(requests.len());
            requests
                .iter()
                .map(|r| wrapped(&r.text, r.max_width))
                .collect()
        }

        fn emit_ops(&self, _: &[u8]) {}
    }

    struct CardsApp;

    impl App for CardsApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            (0..12).fold(column().width(300.0).height(900.0), |list, i| {
                list.child(
                    crate::element::row()
                        .width(-1.0)
                        .child(text(format!("Restaurant number {i}")).weight(1.0))
                        .child(text(format!("{i} km")))
                        .child(button("Directions and a long label that wraps")),
                )
            })
        }

        fn update(&mut self, _: Event) {}
    }

    #[test]
    fn texts_are_measured_in_batches_and_exactly() {
        let viewport = Viewport {
            width: 300.0,
            height: 900.0,
        };
        let host = std::sync::Arc::new(BatchHost {
            batches: Default::default(),
            singles: Default::default(),
        });
        let mut runtime = Runtime::new(CardsApp);
        runtime.set_text_measurer(Rc::new(crate::bridge::CachedTextMeasurer::new(
            std::sync::Arc::clone(&host) as std::sync::Arc<dyn crate::bridge::Host>,
        )));
        runtime.render(viewport);
        let batches = host.batches.lock().unwrap().len();
        assert!((1..=3).contains(&batches), "{batches} batches");
        assert_eq!(host.singles.load(std::sync::atomic::Ordering::SeqCst), 0);

        // The same frames as measuring each text as layout asks.
        struct Direct;
        impl TextMeasurer for Direct {
            fn measure(&self, text: &str, _: f32, _: bool, max_width: f32) -> (f32, f32) {
                wrapped(text, max_width)
            }
        }
        let mut direct = Runtime::new(CardsApp);
        direct.set_text_measurer(Rc::new(Direct));
        direct.render(viewport);
        assert_eq!(runtime.frames, direct.frames);

        // Known texts are never asked about again.
        runtime.refresh();
        assert_eq!(host.batches.lock().unwrap().len(), batches);
    }

    struct ColoredTextApp {
        red: bool,
    }

    impl App for ColoredTextApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            text("hello").color(if self.red { "#ff0000" } else { "#0000ff" })
        }

        fn update(&mut self, _: Event) {
            self.red = !self.red;
        }
    }

    struct FixedButtonApp {
        label: &'static str,
    }

    impl App for FixedButtonApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            column()
                .child(text("measure me"))
                .child(button(self.label).width(50.0).height(20.0))
        }

        fn update(&mut self, _: Event) {
            self.label = if self.label == "off" { "on" } else { "off" };
        }
    }

    struct CallingApp {
        calls: Vec<NativeCall>,
    }

    impl App for CallingApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            column().width(300.0).height(600.0).child(text("hello"))
        }

        fn update(&mut self, _: Event) {
            self.calls.push(NativeCall::new("haptics", "light", "{}"));
        }

        fn take_native_calls(&mut self) -> Vec<NativeCall> {
            std::mem::take(&mut self.calls)
        }
    }

    #[test]
    fn host_info_is_parsed_from_the_host_payload() {
        let info: HostInfo = serde_json::from_str(
            r#"{
                "safeTop": 59.0,
                "safeBottom": 34.0,
                "keyboardInset": 336.0,
                "overlayChrome": true,
                "glass": true,
                "scale": 3.0
            }"#,
        )
        .expect("host info");

        assert_eq!(info.safe_top, 59.0);
        assert_eq!(info.safe_bottom, 34.0);
        assert_eq!(info.keyboard_inset, 336.0);
        assert!(info.overlay_chrome);
        assert!(info.glass);
        assert_eq!(info.scale, 3.0);
    }

    #[test]
    fn host_info_defaults_are_conservative() {
        let info = HostInfo::default();
        assert!(!info.overlay_chrome);
        assert!(!info.glass);
        assert_eq!(info.scale, 1.0);
    }

    #[test]
    fn unchanged_frame_skips_layout() {
        let measurer = Rc::new(CountingMeasurer {
            calls: Cell::new(0),
        });
        let mut runtime = Runtime::new(StaticApp);
        runtime.set_text_measurer(measurer.clone());
        let viewport = Viewport {
            width: 300.0,
            height: 600.0,
        };

        let first = runtime.render(viewport);
        assert!(!first.is_empty());
        let calls_after_first = measurer.calls.get();
        assert!(calls_after_first > 0);

        let second = runtime.render(viewport);
        assert!(second.is_empty());
        assert_eq!(measurer.calls.get(), calls_after_first);
    }

    #[test]
    fn non_layout_prop_change_skips_layout() {
        let measurer = Rc::new(CountingMeasurer {
            calls: Cell::new(0),
        });
        let mut runtime = Runtime::new(ColoredTextApp { red: false });
        runtime.set_text_measurer(measurer.clone());
        let viewport = Viewport {
            width: 300.0,
            height: 600.0,
        };

        let first = runtime.render(viewport);
        assert!(!first.is_empty());
        let calls_after_first = measurer.calls.get();

        let ops = runtime.dispatch(Event {
            id: 0,
            key: None,
            kind: "click".into(),
            payload: String::new(),
        });

        assert!(!ops.is_empty());
        assert_eq!(measurer.calls.get(), calls_after_first);
    }

    #[test]
    fn fixed_size_text_change_skips_layout() {
        let measurer = Rc::new(CountingMeasurer {
            calls: Cell::new(0),
        });
        let mut runtime = Runtime::new(FixedButtonApp { label: "off" });
        runtime.set_text_measurer(measurer.clone());
        let viewport = Viewport {
            width: 300.0,
            height: 600.0,
        };

        let first = runtime.render(viewport);
        assert!(!first.is_empty());
        let calls_after_first = measurer.calls.get();
        assert!(calls_after_first > 0);

        let ops = runtime.dispatch(Event {
            id: 0,
            key: None,
            kind: "click".into(),
            payload: String::new(),
        });

        assert!(!ops.is_empty());
        assert_eq!(measurer.calls.get(), calls_after_first);
    }

    /// Stands in for a hot patch: the "new code" renders different text.
    struct PatchedApp {
        label: &'static str,
    }

    impl App for PatchedApp {
        fn view(&self, _: &crate::app::ViewContext) -> Element {
            column().width(300.0).height(600.0).child(text(self.label))
        }

        fn update(&mut self, _: Event) {}

        fn hot_reloaded(&mut self) {
            self.label = "after patch";
        }
    }

    #[test]
    fn rerender_after_hot_reload_sends_only_the_change() {
        let mut runtime = Runtime::new(PatchedApp {
            label: "before patch",
        });
        let viewport = Viewport {
            width: 300.0,
            height: 600.0,
        };
        assert!(!runtime.render(viewport).is_empty());
        assert!(runtime.render(viewport).is_empty());

        let ops = runtime.rerender();
        assert!(ops
            .iter()
            .any(|op| matches!(op, Op::Text { text, .. } if text == "after patch")));
        assert!(!ops.iter().any(|op| matches!(op, Op::Create { .. })));
    }

    #[test]
    fn native_calls_are_appended_to_unchanged_frames() {
        let mut runtime = Runtime::new(CallingApp { calls: Vec::new() });
        let viewport = Viewport {
            width: 300.0,
            height: 600.0,
        };

        let first = runtime.render(viewport);
        assert!(!first.is_empty());

        let ops = runtime.dispatch(Event {
            id: 0,
            key: None,
            kind: "click".into(),
            payload: String::new(),
        });
        assert_eq!(ops.len(), 1);
        assert!(matches!(
            &ops[0],
            Op::Invoke { module, method, .. }
                if module == "haptics" && method == "light"
        ));
    }
}
