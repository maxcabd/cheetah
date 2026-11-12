use crate::element::{Kind, Props, SourceLocation};
use crate::tree::FlatTree;
use serde::Serialize;

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
}

fn only_text_diff(previous: &Props, next: &Props) -> bool {
    let mut candidate = previous.clone();
    candidate.text = next.text.clone();
    candidate == *next
}

pub fn diff(old: Option<&FlatTree>, new: &FlatTree) -> Vec<Op> {
    let mut creates = Vec::new();
    let mut updates = Vec::new();

    for (id, node) in new {
        match old.and_then(|tree| tree.get(id)) {
            None => creates.push(Op::Create {
                id: *id,
                kind: node.kind,
                props: node.props.clone(),
                source: node.source.clone(),
                root: node.parent.is_none(),
            }),
            Some(previous) if previous.kind != node.kind || previous.props != node.props => {
                if previous.kind != node.kind {
                    updates.push(Op::Update {
                        id: *id,
                        props: node.props.clone(),
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
                        props: node.props.clone(),
                    });
                }
            }
            Some(_) => {}
        }
    }

    let mut child_updates = Vec::new();
    for (id, node) in new {
        let changed = old
            .and_then(|tree| tree.get(id))
            .map(|previous| previous.children != node.children)
            .unwrap_or(true);
        if changed {
            child_updates.push(Op::Children {
                id: *id,
                children: node.children.clone(),
            });
        }
    }

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
