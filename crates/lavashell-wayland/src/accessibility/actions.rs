// SPDX-License-Identifier: GPL-3.0-only
use super::{ids::Ids, text};
use accesskit::{Action, ActionData, TreeId};
use lavashell_core::accessibility::{
    AccessibilityAction as Portable, AccessibilityTree, AccessibleNode, AccessibleValue,
    ActionRequest, TextSelection,
};
use lavashell_core::prelude::Axis;

pub(super) fn request(
    request: accesskit::ActionRequest,
    tree: &AccessibilityTree,
    ids: &Ids,
) -> Option<ActionRequest> {
    if request.target_tree != TreeId::ROOT {
        return None;
    }
    let (target, run) = ids.owner(request.target_node)?;
    if run.is_some() {
        return None;
    }
    let node = tree.nodes.get(&target)?;
    let action = match request.action {
        Action::Click => Portable::Activate,
        Action::Focus => Portable::Focus,
        Action::Increment => Portable::Increment,
        Action::Decrement => Portable::Decrement,
        Action::ScrollIntoView => Portable::ScrollIntoView,
        Action::ScrollUp => scroll(
            tree,
            node,
            request.data.as_ref(),
            Axis::Vertical,
            Toward::Start,
        )?,
        Action::ScrollDown => scroll(
            tree,
            node,
            request.data.as_ref(),
            Axis::Vertical,
            Toward::End,
        )?,
        Action::ScrollLeft => scroll(
            tree,
            node,
            request.data.as_ref(),
            Axis::Horizontal,
            Toward::Start,
        )?,
        Action::ScrollRight => scroll(
            tree,
            node,
            request.data.as_ref(),
            Axis::Horizontal,
            Toward::End,
        )?,
        Action::SetValue => Portable::SetValue(match request.data? {
            ActionData::Value(value) => AccessibleValue::Text(value.into()),
            ActionData::NumericValue(value) if value.is_finite() => AccessibleValue::Number(value),
            _ => return None,
        }),
        Action::ReplaceSelectedText => match request.data? {
            ActionData::Value(value) => Portable::ReplaceSelectedText(value.into()),
            _ => return None,
        },
        Action::SetTextSelection => match request.data? {
            ActionData::SetTextSelection(selection) => Portable::SetTextSelection(TextSelection {
                anchor: text::scalar_position(selection.anchor, node, ids)?,
                focus: text::scalar_position(selection.focus, node, ids)?,
            }),
            _ => return None,
        },
        _ => return None,
    };
    if node.state.semantics.disabled || !node.actions.contains(&action.kind()) {
        return None;
    }
    Some(ActionRequest { target, action })
}

#[derive(Clone, Copy)]
enum Toward {
    Start,
    End,
}

/// Returns a scroll of `node` along `axis` by one page or one item. A page is the extent of `node` along `axis`.
/// An item is the extent of the first child of `node` along `axis`. A request without a unit scrolls by an item.
fn scroll(
    tree: &AccessibilityTree,
    node: &AccessibleNode,
    data: Option<&ActionData>,
    axis: Axis,
    toward: Toward,
) -> Option<Portable> {
    let unit = match data {
        Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Page)) => {
            axis.length(node.bounds.size())
        }
        None | Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Item)) => {
            let item = tree.nodes.get(node.children.first()?)?;
            axis.length(item.bounds.size())
        }
        _ => return None,
    };
    Some(Portable::Scroll(
        axis,
        match toward {
            Toward::Start => -unit,
            Toward::End => unit,
        },
    ))
}
