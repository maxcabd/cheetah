use crate::diff::{diff, Op};
use crate::element::{Element, Props};
use crate::layout::{compute, Frame, TextMeasurer};
use crate::tree::{build, flatten, FlatTree};
use serde::Deserialize;
use std::collections::HashMap;
use std::rc::Rc;

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

pub enum Action {
    SendMessage { id: u32, body: String },
}

pub struct ViewContext<'a> {
    pub viewport: Viewport,
    measurer: Option<&'a dyn TextMeasurer>,
}

impl<'a> ViewContext<'a> {
    pub fn new(viewport: Viewport, measurer: Option<&'a dyn TextMeasurer>) -> Self {
        Self { viewport, measurer }
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
}

pub struct Runtime {
    app: Box<dyn App>,
    previous: Option<FlatTree>,
    frames: HashMap<u32, Frame>,
    viewport: Viewport,
    text_measurer: Option<Rc<dyn TextMeasurer>>,
    layout_dirty: bool,
}

impl Runtime {
    pub fn new<A: App + 'static>(app: A) -> Self {
        Self {
            app: Box::new(app),
            previous: None,
            frames: HashMap::new(),
            viewport: Viewport::default(),
            text_measurer: None,
            layout_dirty: true,
        }
    }

    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.text_measurer = Some(measurer);
        self.layout_dirty = true;
    }

    pub fn render(&mut self, viewport: Viewport) -> Vec<Op> {
        let root = {
            let context = ViewContext::new(viewport, self.text_measurer.as_deref());
            build(self.app.view(&context))
        };
        let flat = flatten(&root);

        // Scrolling and typing still rebuild the view tree, but an unchanged
        // tree with an unchanged viewport must not pay for Taffy again.
        if !self.layout_dirty && viewport == self.viewport && self.previous.as_ref() == Some(&flat)
        {
            return Vec::new();
        }

        let mut ops = diff(self.previous.as_ref(), &flat);
        let layout_needed = self.layout_dirty
            || viewport != self.viewport
            || self
                .previous
                .as_ref()
                .is_none_or(|previous| layout_required(previous, &flat));

        if layout_needed {
            self.viewport = viewport;
            self.layout_dirty = false;
            let frames = compute(
                &root,
                viewport.width,
                viewport.height,
                self.text_measurer.as_deref(),
            );

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
        ops
    }

    pub fn event(&mut self, id: u32, kind: &str, payload: &str) -> Vec<Op> {
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

        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Vec::new();
        }

        self.render(self.viewport)
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

fn props_affect_layout(previous: &Props, next: &Props) -> bool {
    if previous.width != next.width
        || previous.height != next.height
        || previous.pad != next.pad
        || previous.pad_h != next.pad_h
        || previous.pad_v != next.pad_v
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
    use crate::element::{button, column, text, Element};
    use crate::layout::TextMeasurer;
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
}
