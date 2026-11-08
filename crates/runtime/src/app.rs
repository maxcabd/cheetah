use crate::diff::{diff, Op};
use crate::element::Element;
use crate::layout::{compute, Frame};
use crate::tree::{build, flatten, FlatTree};
use std::collections::HashMap;

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

pub trait App {
    fn view(&self, viewport: Viewport) -> Element;
    fn update(&mut self, event: Event);
}

pub struct Runtime {
    app: Box<dyn App>,
    previous: Option<FlatTree>,
    frames: HashMap<u32, Frame>,
    viewport: Viewport,
}

impl Runtime {
    pub fn new<A: App + 'static>(app: A) -> Self {
        Self {
            app: Box::new(app),
            previous: None,
            frames: HashMap::new(),
            viewport: Viewport::default(),
        }
    }

    pub fn render(&mut self, viewport: Viewport) -> Vec<Op> {
        self.viewport = viewport;
        let root = build(self.app.view(viewport));
        let flat = flatten(&root);
        let frames = compute(&root, viewport.width, viewport.height);

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
