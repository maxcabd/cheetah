use runtime::diff::diff;
use runtime::{build, column, flatten, text, Element, Op};

fn flat(element: Element) -> runtime::FlatTree {
    flatten(&build(element))
}

#[test]
fn first_render_creates_root_and_children() {
    let tree = flat(column().child(text("hello")));
    let ops = diff(None, &tree);

    let creates = ops
        .iter()
        .filter(|op| matches!(op, Op::Create { .. }))
        .count();
    assert_eq!(creates, 2);

    let root = ops
        .iter()
        .any(|op| matches!(op, Op::Create { root: true, .. }));
    assert!(root);

    let children = ops
        .iter()
        .filter(|op| matches!(op, Op::Children { .. }))
        .count();
    assert_eq!(children, 2);
}

#[test]
fn unchanged_tree_emits_nothing() {
    let first = flat(column().child(text("hello")));
    let second = flat(column().child(text("hello")));
    let ops = diff(Some(&first), &second);
    assert!(ops.is_empty());
}

#[test]
fn unkeyed_text_change_is_a_text_update() {
    let first = flat(column().child(text("hello")));
    let second = flat(column().child(text("goodbye")));
    let ops = diff(Some(&first), &second);

    assert!(ops.iter().any(|op| matches!(op, Op::Text { .. })));
    assert!(!ops.iter().any(|op| matches!(op, Op::Create { .. })));
}

#[test]
fn keyed_children_keep_identity_when_the_middle_changes() {
    let first = flat(
        column()
            .child(text("a").key("a"))
            .child(text("b").key("b"))
            .child(text("c").key("c")),
    );
    let second = flat(
        column()
            .child(text("a").key("a"))
            .child(text("x").key("x"))
            .child(text("c").key("c")),
    );
    let ops = diff(Some(&first), &second);

    assert!(ops.iter().any(|op| matches!(op, Op::Create { .. })));
    assert!(ops.iter().any(|op| matches!(op, Op::Remove { .. })));
    assert!(ops.iter().any(|op| matches!(op, Op::Children { .. })));
}

#[test]
fn style_change_is_a_sparse_property_update() {
    let first = flat(column().child(text("hello").color("#ffffff").bg("#000000")));
    let second = flat(column().child(text("hello").color("#ffffff").bg("#111111")));
    let ops = diff(Some(&first), &second);

    let props = ops
        .iter()
        .find_map(|op| match op {
            Op::UpdateProps { props, .. } => Some(props),
            _ => None,
        })
        .expect("sparse update");
    assert_eq!(props.bg.as_deref(), Some("#111111"));
    assert!(props.color.is_none());
    assert!(props.text.is_none());
}

#[test]
fn removed_property_falls_back_to_a_full_update() {
    let first = flat(column().child(text("hello").bg("#000000")));
    let second = flat(column().child(text("hello")));
    let ops = diff(Some(&first), &second);

    assert!(ops.iter().any(|op| matches!(op, Op::Update { .. })));
    assert!(!ops.iter().any(|op| matches!(op, Op::UpdateProps { .. })));
}
