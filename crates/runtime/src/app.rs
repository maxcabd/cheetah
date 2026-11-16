use crate::diff::{diff, Op};
use crate::element::Kind;
use crate::element::{Element, Props};
use crate::layout::{compute, Frame, TextMeasurer, TextRequest};
use crate::native_api::NativeCall;
use crate::tree::{build, flatten, FlatTree, UiNode};
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
        _ => return None,
    })
}

pub enum Action {
    SendMessage { id: u32, body: String },
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
            #[cfg(feature = "devserver")]
            literals: Default::default(),
            views,
        }
    }

    pub fn metrics(&self) -> RuntimeMetrics {
        self.metrics
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
            if let Some(measurer) = self.text_measurer.as_deref() {
                let mut requests = Vec::new();
                collect_text_requests(&root, viewport.width, &mut requests);
                self.metrics.text_requests = requests.len();
                let prime_start = Instant::now();
                measurer.prime(&requests);
                self.metrics.prime_us = prime_start.elapsed().as_micros() as u64;
            }
            let layout_start = Instant::now();
            let frames = compute(
                &root,
                viewport.width,
                viewport.height,
                self.text_measurer.as_deref(),
            );
            self.metrics.layout_us = layout_start.elapsed().as_micros() as u64;

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
        let key = self
            .previous
            .as_ref()
            .and_then(|tree| tree.get(&id))
            .and_then(|node| node.key.clone());

        self.app.update(Event {
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
        self.app.update(event);
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

fn collect_text_requests(node: &UiNode, default_width: f32, out: &mut Vec<TextRequest>) {
    if matches!(node.kind, Kind::Text | Kind::Button) {
        if let Some(text) = &node.props.text {
            if !text.is_empty() {
                let width = match node.props.width {
                    Some(width) if width > 0.0 => width,
                    _ => default_width,
                };
                out.push(TextRequest {
                    text: text.clone(),
                    font_size: node.props.size.unwrap_or(14.0),
                    bold: node.props.bold.unwrap_or(false),
                    max_width: width.max(1.0),
                });
            }
        }
    }
    for child in &node.children {
        collect_text_requests(child, default_width, out);
    }
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
