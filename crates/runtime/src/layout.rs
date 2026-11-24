use crate::element::{Kind, Props};
use crate::tree::UiNode;
use serde::Serialize;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRequest {
    pub text: String,
    pub font_size: f32,
    pub bold: bool,
    pub max_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct MeasureCtx {
    text: Option<String>,
    font_size: f32,
    bold: bool,
}

pub trait TextMeasurer {
    /// Returns the content size in dp, excluding padding.
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32);

    /// The size if it's known without asking the host. The default measures,
    /// so a custom measurer needs nothing else.
    fn known(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> Option<(f32, f32)> {
        Some(self.measure(text, font_size, bold, max_width))
    }

    /// Make these known, in one batch where the host can. The default keeps
    /// custom measurers simple.
    fn prime(&self, requests: &[TextRequest]) {
        for request in requests {
            let _ = self.measure(
                &request.text,
                request.font_size,
                request.bold,
                request.max_width,
            );
        }
    }
}

/// The width layout asks for when nothing limits it.
pub(crate) const UNLIMITED_WIDTH: f32 = 100_000.0;

/// Layout passes that may find texts to measure. An estimate can change the
/// width another text gets, so one batch isn't always the last; two or three
/// settle any real screen.
const MEASURE_PASSES: usize = 5;

/// What measuring during a layout took.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Measured {
    pub texts: usize,
    pub time: std::time::Duration,
}

/// Known sizes, and estimates (noted as missing) for the rest.
struct Collector<'a> {
    measurer: &'a dyn TextMeasurer,
    missing: RefCell<Vec<TextRequest>>,
    /// The last size given was an estimate.
    estimated: Cell<bool>,
}

impl TextMeasurer for Collector<'_> {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        if let Some(size) = self.measurer.known(text, font_size, bold, max_width) {
            return size;
        }
        self.estimated.set(true);
        let request = TextRequest {
            text: text.to_string(),
            font_size,
            bold,
            max_width,
        };
        let mut missing = self.missing.borrow_mut();
        // Also unlimited: on one line, that answers every wider question.
        if max_width < UNLIMITED_WIDTH {
            let unlimited = TextRequest {
                max_width: UNLIMITED_WIDTH,
                ..request.clone()
            };
            if self.measurer.known(text, font_size, bold, UNLIMITED_WIDTH).is_none()
                && !missing.contains(&unlimited)
            {
                missing.push(unlimited);
            }
        }
        if !missing.contains(&request) {
            missing.push(request);
        }
        rough_measure(text, font_size, max_width)
    }
}

/// Lay out `root` from nothing.
pub fn compute(
    root: &UiNode,
    width: f32,
    height: f32,
    measurer: Option<&dyn TextMeasurer>,
) -> HashMap<u32, Frame> {
    let mut tree = LayoutTree::default();
    match measurer {
        Some(measurer) => tree.compute(root, width, height, measurer).0,
        None => tree.compute(root, width, height, &Rough).0,
    }
}

/// Sizes without a host's fonts.
pub(crate) struct Rough;

impl TextMeasurer for Rough {
    fn measure(&self, text: &str, font_size: f32, _: bool, max_width: f32) -> (f32, f32) {
        rough_measure(text, font_size, max_width)
    }
}

/// The layout, kept between frames: only nodes whose layout inputs changed,
/// and the ones around them, are laid out again.
pub(crate) struct LayoutTree {
    taffy: TaffyTree<MeasureCtx>,
    nodes: HashMap<u32, LayoutNode>,
    generation: u64,
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self {
            taffy: TaffyTree::new(),
            nodes: HashMap::new(),
            generation: 0,
        }
    }
}

struct LayoutNode {
    node: NodeId,
    kind: Kind,
    props: Rc<Props>,
    children: Vec<u32>,
    /// The last tree it was in.
    generation: u64,
}

impl LayoutTree {
    /// Lay out `root`, asking the host for unknown texts in batches. Each
    /// pass lays out with estimates for them and collects what it asked for,
    /// at the widths it really asked at; a host call per text (a network
    /// round trip on the dev server) would stall layout on every one.
    pub fn compute(
        &mut self,
        root: &UiNode,
        width: f32,
        height: f32,
        measurer: &dyn TextMeasurer,
    ) -> (HashMap<u32, Frame>, Measured) {
        self.generation += 1;
        let root_node = self.sync(root);
        self.remove_unseen();

        let mut measured = Measured::default();
        for _ in 0..MEASURE_PASSES {
            let collector = Collector {
                measurer,
                missing: RefCell::new(Vec::new()),
                estimated: Cell::new(false),
            };
            let mut estimated = Vec::new();
            self.run(root_node, width, height, &collector, |node| {
                if collector.estimated.take() {
                    estimated.push(node);
                }
            });
            let missing = collector.missing.into_inner();
            if missing.is_empty() {
                return (self.frames(root_node, root), measured);
            }
            let start = std::time::Instant::now();
            measurer.prime(&missing);
            measured.time += start.elapsed();
            measured.texts += missing.len();
            // Their sizes were made up: don't keep them.
            for node in estimated {
                self.taffy.mark_dirty(node).expect("taffy node");
            }
        }
        // Still finding texts: ask for the rest as layout needs them.
        self.run(root_node, width, height, measurer, |_| {});
        (self.frames(root_node, root), measured)
    }

    fn run(
        &mut self,
        root: NodeId,
        width: f32,
        height: f32,
        measurer: &dyn TextMeasurer,
        mut measured: impl FnMut(NodeId),
    ) {
        let available = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };
        self.taffy
            .compute_layout_with_measure(root, available, |inputs, node_id, node_context, style| {
                let output = taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known_dimensions, available_space| {
                        measure(known_dimensions, available_space, node_context, measurer)
                    },
                );
                measured(node_id);
                output
            })
            .expect("taffy layout failed");
    }

    /// Bring the taffy tree up to `node`, changing as little as possible:
    /// any change throws away the cached layout of the node and everything
    /// above it.
    fn sync(&mut self, node: &UiNode) -> NodeId {
        let generation = self.generation;
        let children: Vec<NodeId> = node.children.iter().map(|child| self.sync(child)).collect();
        let is_leaf = children.is_empty();

        // A container that becomes a leaf keeps its list of absolute children
        // in taffy 0.15, though they're gone; laying it out then panics. Start
        // such a node over (its parent sees a new child).
        if let Some(known) = self.nodes.get(&node.id) {
            if known.children.is_empty() != is_leaf {
                let stale = known.node;
                self.nodes.remove(&node.id);
                self.taffy.remove(stale).expect("taffy node");
            }
        }

        let Some(known) = self.nodes.get_mut(&node.id) else {
            let style = style_for(node.kind, &node.props);
            let id = if is_leaf {
                self.taffy
                    .new_leaf_with_context(style, measure_context(node.kind, &node.props))
            } else {
                self.taffy.new_with_children(style, &children)
            }
            .expect("taffy node");
            self.nodes.insert(
                node.id,
                LayoutNode {
                    node: id,
                    kind: node.kind,
                    props: Rc::clone(&node.props),
                    children: node.children.iter().map(|child| child.id).collect(),
                    generation,
                },
            );
            return id;
        };

        known.generation = generation;
        let id = known.node;
        let changed = known.kind != node.kind
            || !(Rc::ptr_eq(&known.props, &node.props) || known.props == node.props);
        if changed {
            let style = style_for(node.kind, &node.props);
            if self.taffy.style(id).expect("taffy node") != &style {
                self.taffy.set_style(id, style).expect("taffy node");
            }
            known.kind = node.kind;
            known.props = Rc::clone(&node.props);
        }
        if changed && is_leaf {
            let context = measure_context(node.kind, &node.props);
            if self.taffy.get_node_context(id) != Some(&context) {
                self.taffy
                    .set_node_context(id, Some(context))
                    .expect("taffy node");
            }
        }
        // A child may be a new taffy node under the same id, so compare those.
        if self.taffy.children(id).expect("taffy node") != children {
            self.taffy.set_children(id, &children).expect("taffy node");
        }
        known.children.clear();
        known.children.extend(node.children.iter().map(|child| child.id));
        id
    }

    fn remove_unseen(&mut self) {
        let generation = self.generation;
        let taffy = &mut self.taffy;
        self.nodes.retain(|_, known| {
            let seen = known.generation == generation;
            if !seen {
                taffy.remove(known.node).expect("taffy node");
            }
            seen
        });
    }

    fn frames(&self, root_node: NodeId, root: &UiNode) -> HashMap<u32, Frame> {
        let mut frames = HashMap::with_capacity(self.nodes.len());
        self.collect(root_node, root, &mut frames);
        frames
    }

    fn collect(&self, node: NodeId, ui: &UiNode, frames: &mut HashMap<u32, Frame>) {
        let layout = self.taffy.layout(node).expect("taffy layout result");
        frames.insert(
            ui.id,
            Frame {
                x: layout.location.x,
                y: layout.location.y,
                width: layout.size.width,
                height: layout.size.height,
            },
        );
        for child in &ui.children {
            if let Some(known) = self.nodes.get(&child.id) {
                self.collect(known.node, child, frames);
            }
        }
    }
}

fn style_for(kind: Kind, props: &Props) -> Style {
    let (pad_h, pad_v) = padding(props);
    let mut style = Style {
        display: Display::Flex,
        flex_direction: match kind {
            Kind::Row | Kind::HScroll => FlexDirection::Row,
            _ => FlexDirection::Column,
        },
        size: Size {
            width: dimension(props.width),
            height: dimension(props.height),
        },
        padding: Rect {
            left: LengthPercentage::length(pad_h),
            right: LengthPercentage::length(pad_h),
            top: LengthPercentage::length(pad_v),
            bottom: LengthPercentage::length(props.pad_bottom.unwrap_or(pad_v)),
        },
        ..Default::default()
    };

    if let Some(gap) = props.gap {
        style.gap = Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        };
    }

    if props.wrap.unwrap_or(false) {
        style.flex_wrap = FlexWrap::Wrap;
    }

    if let Some(weight) = props.weight {
        style.flex_grow = weight;
        style.flex_basis = Dimension::length(0.0);
        style.flex_shrink = 1.0;
    } else {
        style.flex_shrink = 0.0;
    }

    style.min_size = Size {
        width: LengthPercentageAuto::length(0.0),
        height: LengthPercentageAuto::auto(),
    };

    style.align_items = align_items(kind, props);
    if let Some(justify) = props.justify.as_deref() {
        style.justify_content = justify_content(justify);
    }

    if props.absolute.unwrap_or(false) {
        style.position = Position::Absolute;
        let top = if props.bottom.is_some() && props.top.is_none() {
            LengthPercentageAuto::auto()
        } else {
            LengthPercentageAuto::length(props.top.unwrap_or(0.0))
        };
        // `right` without `left` pins the right edge instead of the left.
        let left = if props.right.is_some() && props.left.is_none() {
            LengthPercentageAuto::auto()
        } else {
            LengthPercentageAuto::length(props.left.unwrap_or(0.0))
        };
        style.inset = Rect {
            left,
            right: props
                .right
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            top,
            bottom: props
                .bottom
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
        };
    }

    if matches!(
        kind,
        Kind::List | Kind::Scroll | Kind::HScroll | Kind::Glass | Kind::Sheet
    ) {
        style.overflow = Point {
            x: Overflow::Hidden,
            y: Overflow::Hidden,
        };
    }

    style
}

fn dimension(value: Option<f32>) -> Dimension {
    match value {
        Some(-1.0) => Dimension::percent(1.0),
        Some(value) if value >= 0.0 => Dimension::length(value),
        _ => Dimension::auto(),
    }
}

fn padding(props: &Props) -> (f32, f32) {
    let base = props.pad.unwrap_or(0.0);
    (props.pad_h.unwrap_or(base), props.pad_v.unwrap_or(base))
}

fn align_items(kind: Kind, props: &Props) -> AlignItems {
    if let Some(value) = props.align_items.as_deref() {
        return match value {
            "start" => AlignItems::START,
            "center" => AlignItems::CENTER,
            "end" => AlignItems::END,
            "stretch" => AlignItems::STRETCH,
            _ => AlignItems::STRETCH,
        };
    }

    match kind {
        Kind::Row => AlignItems::CENTER,
        _ => AlignItems::STRETCH,
    }
}

fn justify_content(value: &str) -> JustifyContent {
    match value {
        "center" => JustifyContent::CENTER,
        "end" => JustifyContent::END,
        "space_between" | "space-between" => JustifyContent::SPACE_BETWEEN,
        _ => JustifyContent::START,
    }
}

fn measure_context(kind: Kind, props: &Props) -> MeasureCtx {
    let text = match kind {
        Kind::Text | Kind::Button => props.text.clone(),
        Kind::Input => props.placeholder.clone(),
        _ => None,
    };
    MeasureCtx {
        text,
        font_size: props.size.unwrap_or(14.0),
        bold: props.bold.unwrap_or(false),
    }
}

fn measure(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    context: Option<&mut MeasureCtx>,
    measurer: &dyn TextMeasurer,
) -> Size<f32> {
    if let (Some(width), Some(height)) = (known.width, known.height) {
        return Size { width, height };
    }

    let context = match context {
        Some(context) => context,
        None => return Size::ZERO,
    };
    let text = match context.text.as_deref() {
        Some(text) => text,
        None => return Size::ZERO,
    };

    let available_width = known.width.unwrap_or(match available.width {
        AvailableSpace::Definite(width) => width.max(1.0),
        _ => UNLIMITED_WIDTH,
    });

    let (text_width, text_height) =
        measurer.measure(text, context.font_size, context.bold, available_width);

    let width = known
        .width
        .unwrap_or_else(|| text_width.min(available_width));
    let height = known.height.unwrap_or(text_height);

    Size { width, height }
}

pub(crate) fn rough_measure(text: &str, font_size: f32, max_width: f32) -> (f32, f32) {
    let font_size = font_size.max(1.0);
    let line_height = font_size * 1.3;
    let char_width = font_size * 0.55;
    let text_width = text.chars().count() as f32 * char_width;
    let width = text_width.min(max_width);
    let lines = if width > 0.0 {
        (text_width / width).ceil().max(1.0)
    } else {
        1.0
    };
    (width, lines * line_height)
}

/// Inclusive start, exclusive end, with one item of overscan on each side.
pub fn visible_range_prefix(offset: f32, viewport_height: f32, prefix: &[f32]) -> (usize, usize) {
    let count = prefix.len().saturating_sub(1);
    if count == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }

    let total = prefix[count];
    let max_offset = (total - viewport_height).max(0.0);
    let offset = offset.clamp(0.0, max_offset);

    let first = prefix
        .partition_point(|&bottom| bottom <= offset)
        .saturating_sub(1)
        .min(count - 1);
    let end_offset = offset + viewport_height;
    let end_index = prefix.partition_point(|&top| top < end_offset).min(count);

    let start = first.saturating_sub(1);
    let end = (end_index + 1).min(count);
    (start, end)
}

/// Inclusive start, exclusive end, with one item of overscan on each side.
pub fn visible_range(
    offset: f32,
    viewport_height: f32,
    item_height: f32,
    count: usize,
) -> (usize, usize) {
    if count == 0 || item_height <= 0.0 || viewport_height <= 0.0 {
        return (0, 0);
    }

    let raw_start = (offset.max(0.0) / item_height).floor() as usize;
    let start = raw_start.saturating_sub(1).min(count - 1);
    let visible = (viewport_height / item_height).ceil() as usize + 3;
    let end = (start + visible).min(count);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::{compute, visible_range, visible_range_prefix};
    use crate::{build, column, glass, list, row, text};

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    /// Small deterministic random numbers.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    /// A screen-like tree. `seed` decides its shape and `noise` a few of its
    /// details, so trees with the same seed are mostly alike, as frames are.
    fn random_tree(rng: &mut Rng, noise: &mut Rng, depth: u32) -> crate::Element {
        let roll = rng.below(if depth >= 4 { 3 } else { 9 });
        let mut element = match roll {
            0 | 1 => {
                let words = 1 + rng.below(12) + if noise.below(6) == 0 { noise.below(9) } else { 0 };
                text("word ".repeat(words as usize)).size(10.0 + rng.below(12) as f32)
            }
            2 => crate::button("Tap me"),
            3 | 4 => row(),
            5 => list().content_height(400.0),
            6 => glass(),
            _ => column(),
        };
        if roll > 2 {
            let children = rng.below(4) + 1;
            for i in 0..children {
                let mut child = random_tree(rng, noise, depth + 1);
                if noise.below(25) == 0 {
                    continue;
                }
                if rng.below(2) == 0 {
                    child = child.key(format!("k{i}"));
                }
                element = element.child(child);
            }
            if rng.below(3) == 0 {
                element = element.gap(rng.below(20) as f32);
            }
        }
        match rng.below(8) {
            0 => element = element.width(-1.0),
            1 => element = element.width(40.0 + rng.below(200) as f32),
            2 => element = element.weight(1.0),
            3 => element = element.height(20.0 + rng.below(100) as f32),
            4 => element = element.pad(rng.below(16) as f32),
            5 if depth > 0 => element = element.absolute().top(10.0).right(12.0),
            _ => {}
        }
        if noise.below(12) == 0 {
            element = element.pad_h(noise.below(30) as f32);
        }
        if noise.below(12) == 0 {
            element = element.bg("#123456");
        }
        element
    }

    struct Wrapping;

    impl super::TextMeasurer for Wrapping {
        fn measure(&self, text: &str, size: f32, _: bool, max_width: f32) -> (f32, f32) {
            let max_width = max_width.max(1.0);
            let full = text.len() as f32 * size * 0.5;
            (full.min(max_width), size * 1.2 * (full / max_width).ceil().max(1.0))
        }
    }

    /// Like a host with a cache: asked once, known after.
    #[derive(Default)]
    struct Lazy(std::cell::RefCell<std::collections::HashSet<(String, u32, u32)>>);

    impl super::TextMeasurer for Lazy {
        fn measure(&self, text: &str, size: f32, bold: bool, max_width: f32) -> (f32, f32) {
            super::TextMeasurer::measure(&Wrapping, text, size, bold, max_width)
        }

        fn known(&self, text: &str, size: f32, bold: bool, width: f32) -> Option<(f32, f32)> {
            let key = (text.to_string(), size.to_bits(), width.to_bits());
            self.0
                .borrow()
                .contains(&key)
                .then(|| self.measure(text, size, bold, width))
        }

        fn prime(&self, requests: &[super::TextRequest]) {
            let mut known = self.0.borrow_mut();
            for r in requests {
                known.insert((r.text.clone(), r.font_size.to_bits(), r.max_width.to_bits()));
            }
        }
    }

    #[test]
    fn a_kept_layout_matches_one_from_nothing() {
        for seed in 1..=60u64 {
            let mut tree = super::LayoutTree::default();
            let mut lazy_tree = super::LayoutTree::default();
            let lazy = Lazy::default();
            let mut noise = Rng(seed * 7919 + 1);
            for frame in 0..25u64 {
                // Mostly the same screen; sometimes a different one.
                let shape = if frame % 9 == 8 { seed + 1000 + frame } else { seed };
                let root = build(
                    column()
                        .width(-1.0)
                        .height(-1.0)
                        .child(random_tree(&mut Rng(shape * 104_729 + 3), &mut noise, 0))
                        .child(random_tree(&mut Rng(shape * 1_299_709 + 5), &mut noise, 0)),
                );
                let (width, height) = if frame % 11 == 10 { (852.0, 393.0) } else { (393.0, 852.0) };
                let fresh = compute(&root, width, height, Some(&Wrapping));
                let (kept, _) = tree.compute(&root, width, height, &Wrapping);
                assert_eq!(kept, fresh, "seed {seed} frame {frame}");
                let (kept, _) = lazy_tree.compute(&root, width, height, &lazy);
                assert_eq!(kept, fresh, "batched, seed {seed} frame {frame}");
            }
        }
    }

    #[test]
    fn removed_nodes_are_forgotten() {
        let mut tree = super::LayoutTree::default();
        let big = build((0..20).fold(column(), |c, i| c.child(text(format!("{i}")))));
        tree.compute(&big, 300.0, 600.0, &Wrapping);
        assert_eq!(tree.nodes.len(), 21);
        let small = build(column().child(text("0")));
        tree.compute(&small, 300.0, 600.0, &Wrapping);
        assert_eq!((tree.nodes.len(), tree.taffy.total_node_count()), (2, 2));
    }

    #[test]
    fn absolute_chrome_anchors_to_bottom() {
        let root = build(
            column()
                .width(300.0)
                .height(600.0)
                .child(row().width(300.0).height(600.0))
                .child(
                    glass()
                        .absolute()
                        .bottom(0.0)
                        .left(0.0)
                        .width(300.0)
                        .height(68.0),
                ),
        );
        let frames = compute(&root, 300.0, 600.0, None);
        let frame = frames[&root.children[1].id];

        assert_close(frame.x, 0.0);
        assert_close(frame.y, 532.0);
        assert_close(frame.width, 300.0);
        assert_close(frame.height, 68.0);
    }

    #[test]
    fn absolute_right_anchors_to_the_right_edge() {
        let root = build(
            column()
                .width(300.0)
                .height(600.0)
                .child(row().absolute().top(10.0).right(16.0).width(38.0).height(38.0)),
        );
        let frames = compute(&root, 300.0, 600.0, None);
        let frame = frames[&root.children[0].id];
        assert_close(frame.x, 246.0);
        assert_close(frame.y, 10.0);
    }

    #[test]
    fn column_stacks_children() {
        let root = build(
            column()
                .width(300.0)
                .height(600.0)
                .child(text("header").height(64.0))
                .child(row().height(468.0))
                .child(row().height(68.0)),
        );
        let frames = compute(&root, 300.0, 600.0, None);

        let root_frame = frames[&root.id];
        assert_close(root_frame.width, 300.0);
        assert_close(root_frame.height, 600.0);

        let header = &root.children[0];
        let list = &root.children[1];
        let composer = &root.children[2];

        assert_close(frames[&header.id].y, 0.0);
        assert_close(frames[&header.id].height, 64.0);
        assert_close(frames[&list.id].y, 64.0);
        assert_close(frames[&list.id].height, 468.0);
        assert_close(frames[&composer.id].y, 532.0);
        assert_close(frames[&composer.id].height, 68.0);
    }

    #[test]
    fn absolute_rows_are_positioned_in_list_coordinates() {
        let root = build(
            column().width(300.0).height(600.0).child(
                list()
                    .width(-1.0)
                    .height(600.0)
                    .content_height(780.0)
                    .child(
                        row()
                            .absolute()
                            .top(156.0)
                            .left(0.0)
                            .width(-1.0)
                            .height(78.0),
                    ),
            ),
        );
        let frames = compute(&root, 300.0, 600.0, None);
        let item = &root.children[0].children[0];
        let frame = frames[&item.id];

        assert_close(frame.x, 0.0);
        assert_close(frame.y, 156.0);
        assert_close(frame.width, 300.0);
        assert_close(frame.height, 78.0);
    }

    #[test]
    fn prefix_range_handles_variable_heights() {
        let prefix = [0.0, 50.0, 150.0, 200.0, 260.0];
        assert_eq!(visible_range_prefix(0.0, 100.0, &prefix), (0, 3));
        assert_eq!(visible_range_prefix(150.0, 50.0, &prefix), (1, 4));
        assert_eq!(visible_range_prefix(10_000.0, 100.0, &prefix), (1, 4));
        assert_eq!(visible_range_prefix(0.0, 1_000.0, &prefix), (0, 4));
        assert_eq!(visible_range_prefix(0.0, 100.0, &[0.0]), (0, 0));
    }

    #[test]
    fn visible_range_adds_overscan() {
        assert_eq!(visible_range(0.0, 600.0, 80.0, 100), (0, 11));
        assert_eq!(visible_range(800.0, 600.0, 80.0, 100), (9, 20));
    }

    #[test]
    fn visible_range_clamps_at_the_end() {
        assert_eq!(visible_range(10_000.0, 600.0, 80.0, 20), (19, 20));
    }

    #[test]
    fn visible_range_handles_empty_and_zero_sizes() {
        assert_eq!(visible_range(0.0, 600.0, 80.0, 0), (0, 0));
        assert_eq!(visible_range(0.0, 600.0, 0.0, 10), (0, 0));
    }
}
