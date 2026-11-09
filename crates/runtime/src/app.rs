use crate::diff::{diff, Op};
use crate::element::Element;
use crate::layout::{compute, Frame, TextMeasurer};
use crate::tree::{build, flatten, FlatTree};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

pub struct Event {
    pub id: u32,
    pub key: Option<String>,
    pub kind: String,
    pub payload: String,
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
}

pub struct Runtime {
    app: Box<dyn App>,
    previous: Option<FlatTree>,
    frames: HashMap<u32, Frame>,
    viewport: Viewport,
    text_measurer: Option<Rc<dyn TextMeasurer>>,
}

impl Runtime {
    pub fn new<A: App + 'static>(app: A) -> Self {
        Self {
            app: Box::new(app),
            previous: None,
            frames: HashMap::new(),
            viewport: Viewport::default(),
            text_measurer: None,
        }
    }

    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.text_measurer = Some(measurer);
    }

    pub fn render(&mut self, viewport: Viewport) -> Vec<Op> {
        self.viewport = viewport;
        let root = {
            let context = ViewContext::new(viewport, self.text_measurer.as_deref());
            build(self.app.view(&context))
        };
        let flat = flatten(&root);
        let frames = compute(
            &root,
            viewport.width,
            viewport.height,
            self.text_measurer.as_deref(),
        );

        let mut ops = diff(self.previous.as_ref(), &flat);
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

        self.previous = Some(flat);
        self.frames = frames;
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

        self.render(self.viewport)
    }
}
