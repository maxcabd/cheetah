// Fixture for tests/live_edit.rs: the test rewrites the literal below while
// the dev server runs, then restores it.
use runtime::{column, text, Element};

pub fn screen() -> Element {
    column()
        .width(300.0)
        .height(600.0)
        .child(text("live-before").size(14.0))
}
