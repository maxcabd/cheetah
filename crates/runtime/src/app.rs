use crate::diff::{diff, Op};
use crate::element::Element;
use crate::tree::{build, flatten, FlatTree};

pub struct Event {
    pub id: u32,
    pub key: Option<String>,
    pub kind: String,
    pub payload: String,
}

pub trait App {
    fn view(&self) -> Element;
    fn update(&mut self, event: Event);
}

pub struct Runtime {
    app: Box<dyn App>,
    previous: Option<FlatTree>,
}

impl Runtime {
    pub fn new<A: App + 'static>(app: A) -> Self {
        Self {
            app: Box::new(app),
            previous: None,
        }
    }

    pub fn render(&mut self) -> Vec<Op> {
        let root = build(self.app.view());
        let flat = flatten(&root);
        let ops = diff(self.previous.as_ref(), &flat);
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

        self.render()
    }
}
