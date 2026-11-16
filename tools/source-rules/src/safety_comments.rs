// SPDX-License-Identifier: GPL-3.0-only
use crate::grammar::source_of;
use std::collections::BTreeSet;
use tree_sitter::Node;

const SCOPE_ITEMS: [&str; 4] = ["function_item", "mod_item", "impl_item", "trait_item"];

pub fn safety_comments(comments: &[Node<'_>], text: &str) -> BTreeSet<usize> {
    let body = |comment: &Node<'_>| source_of(comment, text);
    let is_doc = |comment: &Node<'_>| body(comment).starts_with("///");
    comments
        .chunk_by(|above, below| {
            below.start_position().row == above.start_position().row + 1
                && body(above).starts_with("//")
                && body(below).starts_with("//")
                && is_doc(above) == is_doc(below)
        })
        .filter(|block| {
            let (Some(first), Some(last)) = (block.first(), block.last()) else {
                return false;
            };
            let below = last
                .next_sibling()
                .filter(|node| node.start_position().row == last.start_position().row + 1);
            if body(first).starts_with("// SAFETY:") {
                below.is_some_and(holds_unsafe_code)
            } else {
                body(first) == "/// # Safety" && below.is_some_and(declares_unsafe_contract)
            }
        })
        .flatten()
        .map(Node::id)
        .collect()
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    (0..node.child_count()).filter_map(move |index| node.child(index))
}

fn is_marked_unsafe(node: Node<'_>) -> bool {
    children(node).any(|child| {
        child.kind() == "unsafe"
            || child.kind() == "function_modifiers"
                && children(child).any(|modifier| modifier.kind() == "unsafe")
    })
}

fn holds_unsafe_code(node: Node<'_>) -> bool {
    node.kind() == "unsafe_block"
        || node.kind() == "impl_item" && is_marked_unsafe(node)
        || !SCOPE_ITEMS.contains(&node.kind()) && children(node).any(holds_unsafe_code)
}

fn declares_unsafe_contract(node: Node<'_>) -> bool {
    let mut item = node;
    while item.kind() == "attribute_item" {
        let Some(next) = item.next_sibling() else {
            return false;
        };
        item = next;
    }
    matches!(
        item.kind(),
        "function_item" | "function_signature_item" | "trait_item"
    ) && is_marked_unsafe(item)
}
