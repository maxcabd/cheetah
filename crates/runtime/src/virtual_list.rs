//! Virtualized lists (the FlatList equivalent): only the rows in and near the
//! viewport exist as native views, so a list of 10,000 rows costs about as
//! much as one screenful.
//!
//! The app keeps a [`VirtualList`] in its state, feeds it scroll events, and
//! renders through it:
//!
//! ```ignore
//! struct App { places: Vec<Place>, list: VirtualList }
//!
//! fn update(&mut self, event: Event) {
//!     if self.list.on_event(&event) { return; } // just a scroll
//! }
//!
//! fn view(&self, context: &ViewContext) -> Element {
//!     self.list.render(self.places.len(), 600.0, |index| {
//!         text(&self.places[index].name).height(72.0)
//!     })
//! }
//! ```
//!
//! Rows are positioned absolutely inside one scroll content, keyed by index
//! (or [`VirtualList::keys`]), so scrolling only adds and removes rows at the
//! edges. Heights are fixed ([`VirtualList::new`]) or per row
//! ([`VirtualList::heights`]).

use crate::app::Event;
use crate::element::{list, Element};
use crate::layout::visible_range_prefix;

pub struct VirtualList {
    key: String,
    row_height: f32,
    offset: f32,
    overscan: usize,
}

impl VirtualList {
    /// A list whose scroll element has `key` and whose rows are `row_height`
    /// points tall.
    pub fn new(key: impl Into<String>, row_height: f32) -> Self {
        Self {
            key: key.into(),
            row_height: row_height.max(1.0),
            offset: 0.0,
            overscan: 4,
        }
    }

    /// Extra rows rendered above and below the viewport (default 4).
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows;
        self
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// Current scroll offset in points.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Take this list's scroll events; true when `event` was one (re-render
    /// to move the window).
    pub fn on_event(&mut self, event: &Event) -> bool {
        if event.kind != "scroll" || event.key.as_deref() != Some(self.key.as_str()) {
            return false;
        }
        if let Ok(offset) = event.payload.trim().parse::<f32>() {
            self.offset = offset.max(0.0);
        }
        true
    }

    /// Rows `start..end` to render for `count` fixed-height rows.
    pub fn window(&self, count: usize, viewport_height: f32) -> (usize, usize) {
        // A stale offset past the end (the list shrank) shows the last screen.
        let max_offset = (count as f32 * self.row_height - viewport_height).max(0.0);
        let first = (self.offset.min(max_offset) / self.row_height).floor() as usize;
        let visible = (viewport_height / self.row_height).ceil() as usize + 1;
        let start = first.saturating_sub(self.overscan).min(count);
        let end = (first + visible + self.overscan).min(count);
        (start, end)
    }

    /// The scroll element for `count` fixed-height rows in a viewport
    /// `viewport_height` tall. `row(index)` builds one row; it is placed and
    /// sized by the list.
    pub fn render(
        &self,
        count: usize,
        viewport_height: f32,
        row: impl Fn(usize) -> Element,
    ) -> Element {
        let (start, end) = self.window(count, viewport_height);
        let mut scroll = list()
            .key(self.key.clone())
            .width(-1.0)
            .height(viewport_height)
            .content_height(count as f32 * self.row_height);
        for index in start..end {
            scroll = scroll.child(self.place(
                row(index),
                index,
                index as f32 * self.row_height,
                self.row_height,
            ));
        }
        scroll
    }

    /// Like [`render`](Self::render) for rows of different heights.
    pub fn heights(
        &self,
        heights: &[f32],
        viewport_height: f32,
        row: impl Fn(usize) -> Element,
    ) -> Element {
        let mut prefix = Vec::with_capacity(heights.len() + 1);
        prefix.push(0.0);
        for height in heights {
            prefix.push(prefix.last().copied().unwrap_or(0.0) + height.max(0.0));
        }
        let (mut start, mut end) = visible_range_prefix(self.offset, viewport_height, &prefix);
        start = start.saturating_sub(self.overscan);
        end = (end + self.overscan).min(heights.len());
        let mut scroll = list()
            .key(self.key.clone())
            .width(-1.0)
            .height(viewport_height)
            .content_height(prefix.last().copied().unwrap_or(0.0));
        for index in start..end {
            scroll = scroll.child(self.place(row(index), index, prefix[index], heights[index]));
        }
        scroll
    }

    fn place(&self, row: Element, index: usize, top: f32, height: f32) -> Element {
        // Keep the app's key when it set one (stable identity across data
        // changes); otherwise the index.
        let key = row
            .key
            .clone()
            .unwrap_or_else(|| format!("{}:{index}", self.key));
        row.key(key)
            .absolute()
            .top(top)
            .left(0.0)
            .width(-1.0)
            .height(height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::text;

    fn scroll(list: &mut VirtualList, offset: f32) {
        let event = Event {
            id: 1,
            key: Some(list.key().to_string()),
            kind: "scroll".into(),
            payload: offset.to_string(),
        };
        assert!(list.on_event(&event));
    }

    #[test]
    fn renders_only_the_window_plus_overscan() {
        let mut list = VirtualList::new("places", 50.0).overscan(2);
        let root = list.render(10_000, 500.0, |i| text(format!("row {i}")));
        assert_eq!(root.children.len(), 13, "11 visible + 2 below");
        assert_eq!(root.props.content_height, Some(500_000.0));

        scroll(&mut list, 5_000.0);
        let root = list.render(10_000, 500.0, |i| text(format!("row {i}")));
        let first = &root.children[0];
        assert_eq!(first.props.text.as_deref(), Some("row 98"));
        assert_eq!(first.props.top, Some(98.0 * 50.0));
        assert_eq!(first.key.as_deref(), Some("places:98"));
        assert_eq!(root.children.len(), 15);
    }

    #[test]
    fn ignores_other_events_and_clamps_at_the_end() {
        let mut list = VirtualList::new("places", 50.0);
        let other = Event {
            id: 1,
            key: Some("other".into()),
            kind: "scroll".into(),
            payload: "90".into(),
        };
        assert!(!list.on_event(&other));
        scroll(&mut list, 1_000_000.0);
        assert_eq!(list.window(20, 500.0), (6, 20));
        assert_eq!(VirtualList::new("x", 50.0).window(0, 500.0), (0, 0));
    }

    #[test]
    fn variable_heights_and_app_keys() {
        let mut list = VirtualList::new("feed", 1.0).overscan(0);
        let heights = vec![100.0; 50];
        scroll(&mut list, 1_000.0);
        let root = list.heights(&heights, 300.0, |i| text("x").key(format!("post-{i}")));
        assert_eq!(root.props.content_height, Some(5_000.0));
        assert!(root
            .children
            .iter()
            .any(|row| row.key.as_deref() == Some("post-10")));
        assert!(!root
            .children
            .iter()
            .any(|row| row.key.as_deref() == Some("post-0")));
    }
}
