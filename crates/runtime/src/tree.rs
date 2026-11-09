use crate::element::{Element, Kind, Props};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    pub id: u32,
    pub key: Option<String>,
    pub kind: Kind,
    pub props: Props,
    pub children: Vec<UiNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlatNode {
    pub id: u32,
    pub key: Option<String>,
    pub kind: Kind,
    pub props: Props,
    pub children: Vec<u32>,
    pub parent: Option<u32>,
}

pub type FlatTree = HashMap<u32, FlatNode>;

const ROOT_ID: u32 = 1;
const ROOT_HASH: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn hash_byte(mut hash: u64, byte: u8) -> u64 {
    hash ^= byte as u64;
    hash.wrapping_mul(FNV_PRIME)
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash = hash_byte(hash, byte);
    }
    hash
}

fn hash_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash = hash_byte(hash, byte);
    }
    hash
}

fn child_identity(parent_hash: u64, kind: Kind, key: Option<&str>, index: usize) -> u64 {
    let mut hash = hash_byte(parent_hash, 0x1f);
    hash = hash_bytes(hash, kind.as_str().as_bytes());
    hash = hash_byte(hash, 0x1f);
    match key {
        Some(key) => {
            hash = hash_byte(hash, b'k');
            hash = hash_bytes(hash, key.as_bytes());
        }
        None => {
            hash = hash_byte(hash, b'i');
            hash = hash_u64(hash, index as u64);
        }
    }
    hash
}

struct IdAllocator {
    used: HashSet<u32>,
}

impl IdAllocator {
    fn new() -> Self {
        let mut used = HashSet::new();
        used.insert(ROOT_ID);
        Self { used }
    }

    fn allocate(&mut self, hash: u64) -> u32 {
        let mut id = (hash as u32) & 0x7fff_ffff;
        if id <= ROOT_ID {
            id = ROOT_ID + 1;
        }
        while !self.used.insert(id) {
            id = id.wrapping_add(1) & 0x7fff_ffff;
            if id <= ROOT_ID {
                id = ROOT_ID + 1;
            }
        }
        id
    }
}

pub fn build(root: Element) -> UiNode {
    let mut allocator = IdAllocator::new();
    build_node(root, ROOT_ID, ROOT_HASH, &mut allocator)
}

fn build_node(element: Element, id: u32, hash: u64, allocator: &mut IdAllocator) -> UiNode {
    let mut children = Vec::with_capacity(element.children.len());
    for (index, child) in element.children.into_iter().enumerate() {
        let child_hash = child_identity(hash, child.kind, child.key.as_deref(), index);
        let child_id = allocator.allocate(child_hash);
        children.push(build_node(child, child_id, child_hash, allocator));
    }

    UiNode {
        id,
        key: element.key,
        kind: element.kind,
        props: element.props,
        children,
    }
}

pub fn flatten(root: &UiNode) -> FlatTree {
    let mut tree = FlatTree::new();
    flatten_node(root, None, &mut tree);
    tree
}

fn flatten_node(node: &UiNode, parent: Option<u32>, tree: &mut FlatTree) {
    tree.insert(
        node.id,
        FlatNode {
            id: node.id,
            key: node.key.clone(),
            kind: node.kind,
            props: node.props.clone(),
            children: node.children.iter().map(|child| child.id).collect(),
            parent,
        },
    );

    for child in &node.children {
        flatten_node(child, Some(node.id), tree);
    }
}
