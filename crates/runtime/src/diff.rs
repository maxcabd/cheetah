use crate::element::{Kind, Props};
use crate::tree::FlatTree;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Create {
        id: u32,
        kind: Kind,
        props: Props,
        root: bool,
    },
    Update {
        id: u32,
        props: Props,
    },
    Children {
        id: u32,
        children: Vec<u32>,
    },
    Remove {
        id: u32,
    },
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
                root: node.parent.is_none(),
            }),
            Some(previous) if previous.kind != node.kind || previous.props != node.props => {
                updates.push(Op::Update {
                    id: *id,
                    props: node.props.clone(),
                });
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
