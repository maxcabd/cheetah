use crate::element::{Kind, Props};
use crate::tree::UiNode;
use serde::Serialize;
use std::collections::HashMap;
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

struct MeasureCtx {
    text: Option<String>,
    font_size: f32,
    bold: bool,
}

pub trait TextMeasurer {
    /// Returns the content size in dp, excluding padding.
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32);
}

pub fn compute(
    root: &UiNode,
    width: f32,
    height: f32,
    measurer: Option<&dyn TextMeasurer>,
) -> HashMap<u32, Frame> {
    let mut taffy: TaffyTree<MeasureCtx> = TaffyTree::new();
    let root_node = build_node(&mut taffy, root);

    let available = Size {
        width: AvailableSpace::Definite(width),
        height: AvailableSpace::Definite(height),
    };

    taffy
        .compute_layout_with_measure(
            root_node,
            available,
            |inputs, _node_id, node_context, style| {
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known_dimensions, available_space| {
                        measure(known_dimensions, available_space, node_context, measurer)
                    },
                )
            },
        )
        .expect("taffy layout failed");

    let mut frames = HashMap::new();
    collect(&taffy, root_node, root, &mut frames);
    frames
}

fn build_node(taffy: &mut TaffyTree<MeasureCtx>, node: &UiNode) -> NodeId {
    let style = style_for(node.kind, &node.props);

    if node.children.is_empty() {
        taffy
            .new_leaf_with_context(style, measure_context(node.kind, &node.props))
            .expect("taffy leaf")
    } else {
        let children: Vec<NodeId> = node
            .children
            .iter()
            .map(|child| build_node(taffy, child))
            .collect();
        taffy
            .new_with_children(style, &children)
            .expect("taffy container")
    }
}

fn style_for(kind: Kind, props: &Props) -> Style {
    let (pad_h, pad_v) = padding(props);
    let mut style = Style {
        display: Display::Flex,
        flex_direction: match kind {
            Kind::Row => FlexDirection::Row,
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
            bottom: LengthPercentage::length(pad_v),
        },
        ..Default::default()
    };

    if let Some(gap) = props.gap {
        style.gap = Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        };
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
        style.inset = Rect {
            left: LengthPercentageAuto::length(props.left.unwrap_or(0.0)),
            right: LengthPercentageAuto::auto(),
            top,
            bottom: props
                .bottom
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
        };
    }

    if matches!(kind, Kind::List | Kind::Scroll | Kind::Glass) {
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
    measurer: Option<&dyn TextMeasurer>,
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
        _ => 100_000.0,
    });

    let (text_width, text_height) = match measurer {
        Some(measurer) => measurer.measure(text, context.font_size, context.bold, available_width),
        None => rough_measure(text, context.font_size, available_width),
    };

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

fn collect(
    taffy: &TaffyTree<MeasureCtx>,
    node: NodeId,
    ui: &UiNode,
    frames: &mut HashMap<u32, Frame>,
) {
    let layout = taffy.layout(node).expect("taffy layout result");
    frames.insert(
        ui.id,
        Frame {
            x: layout.location.x,
            y: layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        },
    );

    let children = taffy.children(node).expect("taffy children");
    for (child_node, child_ui) in children.iter().zip(ui.children.iter()) {
        collect(taffy, *child_node, child_ui, frames);
    }
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
