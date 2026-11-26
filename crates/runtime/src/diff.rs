use crate::element::{Kind, Props, SourceLocation};
use crate::tree::FlatTree;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Create {
        id: u32,
        kind: Kind,
        props: Props,
        source: Option<SourceLocation>,
        root: bool,
    },
    Update {
        id: u32,
        props: Props,
    },
    UpdateProps {
        id: u32,
        props: Props,
    },
    Text {
        id: u32,
        text: String,
    },
    Children {
        id: u32,
        children: Vec<u32>,
    },
    Layout {
        id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Remove {
        id: u32,
    },
    /// Imperative platform call, appended after the view ops for a frame.
    Invoke {
        module: String,
        method: String,
        args: String,
    },
}

fn only_text_diff(previous: &Props, next: &Props) -> bool {
    let mut candidate = previous.clone();
    candidate.text = next.text.clone();
    candidate == *next
}

/// How many parents `id` has, remembered for the nodes on the way up.
fn depth(tree: &FlatTree, id: u32, known: &mut HashMap<u32, usize>) -> usize {
    let mut chain = Vec::new();
    let mut base = 0;
    let mut current = Some(id);
    while let Some(node) = current {
        if let Some(&depth) = known.get(&node) {
            base = depth + 1;
            break;
        }
        chain.push(node);
        current = tree.get(&node).and_then(|n| n.parent);
    }
    let top = chain.len().saturating_sub(1);
    for (index, node) in chain.iter().enumerate() {
        known.insert(*node, base + top - index);
    }
    known[&id]
}

pub fn diff(old: Option<&FlatTree>, new: &FlatTree) -> Vec<Op> {
    let mut creates = Vec::new();
    let mut updates = Vec::new();

    for (id, node) in new {
        match old.and_then(|tree| tree.get(id)) {
            None => creates.push(Op::Create {
                id: *id,
                kind: node.kind,
                props: (*node.props).clone(),
                source: node.source.clone(),
                root: node.parent.is_none(),
            }),
            Some(previous) if previous.kind != node.kind || previous.props != node.props => {
                if previous.kind != node.kind {
                    updates.push(Op::Update {
                        id: *id,
                        props: (*node.props).clone(),
                    });
                } else if only_text_diff(&previous.props, &node.props) {
                    updates.push(Op::Text {
                        id: *id,
                        text: node.props.text.clone().unwrap_or_default(),
                    });
                } else if let Some(props) = node.props.delta_from(&previous.props) {
                    updates.push(Op::UpdateProps { id: *id, props });
                } else {
                    updates.push(Op::Update {
                        id: *id,
                        props: (*node.props).clone(),
                    });
                }
            }
            Some(_) => {}
        }
    }

    // Deepest first: a new subtree is put together before it's attached, so
    // the platform puts each into the window once, not view by view (which
    // costs UIKit a trait and accessibility update per view). The map's own
    // order would be random.
    let mut depths = HashMap::new();
    let mut child_updates = Vec::new();
    for (id, node) in new {
        let changed = old
            .and_then(|tree| tree.get(id))
            .map(|previous| previous.children != node.children)
            .unwrap_or(true);
        if changed {
            child_updates.push((depth(new, *id, &mut depths), *id, node.children.clone()));
        }
    }
    child_updates.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let child_updates = child_updates
        .into_iter()
        .map(|(_, id, children)| Op::Children { id, children });

    let mut removes = Vec::new();
    if let Some(old) = old {
        for id in old.keys() {
            if !new.contains_key(id) {
                removes.push(Op::Remove { id: *id });
            }
        }
    }

    // Creates must land before any children operation, and removes must land
    // after children have been re-pointed at their new nodes.
    let mut ops =
        Vec::with_capacity(creates.len() + updates.len() + child_updates.len() + removes.len());
    ops.extend(creates);
    ops.extend(updates);
    ops.extend(child_updates);
    ops.extend(removes);
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FlatNode;
    use std::rc::Rc;

    fn node(id: u32, parent: Option<u32>, children: Vec<u32>) -> (u32, FlatNode) {
        (
            id,
            FlatNode {
                id,
                key: None,
                kind: Kind::Column,
                props: Rc::new(Props::default()),
                source: None,
                children,
                parent,
            },
        )
    }

    #[test]
    fn attaches_subtrees_deepest_first() {
        let tree: FlatTree = [
            node(1, None, vec![2, 5]),
            node(2, Some(1), vec![3]),
            node(3, Some(2), vec![4]),
            node(4, Some(3), vec![]),
            node(5, Some(1), vec![]),
        ]
        .into_iter()
        .collect();
        let order: Vec<u32> = diff(None, &tree)
            .into_iter()
            .filter_map(|op| match op {
                Op::Children { id, .. } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(order, vec![4, 3, 2, 5, 1]);
    }
}
