// SPDX-License-Identifier: GPL-3.0-only
use crate::keyboard::Key;
use crate::ui::NodeId;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FocusSource {
    Key,
    Pointer,
    Assistive,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct FocusedNode {
    pub node: NodeId,
    pub pressed_key: Option<Key>,
}

#[derive(Default)]
pub(super) struct KeyboardFocus {
    focused: Option<FocusedNode>,
    source: Option<FocusSource>,
    before_modal: Option<NodeId>,
}

impl KeyboardFocus {
    pub fn focused_node(&self) -> Option<FocusedNode> {
        self.focused
    }

    pub fn node(&self) -> Option<NodeId> {
        self.focused.map(|focused| focused.node)
    }

    pub fn pressed_key(&self) -> Option<Key> {
        self.focused?.pressed_key
    }

    pub fn source(&self) -> Option<FocusSource> {
        self.source
    }

    pub fn move_to(&mut self, node: Option<NodeId>) {
        if self.node() != node {
            self.focused = node.map(|node| FocusedNode {
                node,
                pressed_key: None,
            });
        }
    }

    pub fn clear(&mut self) {
        self.move_to(None);
    }

    pub fn move_with_source(&mut self, node: NodeId, source: FocusSource) -> bool {
        self.move_to(Some(node));
        self.record_source(source)
    }

    pub fn record_source(&mut self, source: FocusSource) -> bool {
        let changed = self.source != Some(source);
        self.source = Some(source);
        changed
    }

    pub fn set_pressed_key(&mut self, key: Option<Key>) {
        if let Some(focused) = &mut self.focused {
            focused.pressed_key = key;
        }
    }

    pub fn remember_before_modal(&mut self, node: Option<NodeId>) {
        self.before_modal = node;
    }

    pub fn take_before_modal(&mut self) -> Option<NodeId> {
        self.before_modal.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_to_the_focused_node_keeps_the_key_held_on_it() {
        let mut focus = KeyboardFocus::default();
        focus.move_to(Some(NodeId(1)));
        focus.set_pressed_key(Some(Key::Space));
        focus.move_to(Some(NodeId(1)));
        assert_eq!(focus.pressed_key(), Some(Key::Space));
    }

    #[test]
    fn a_key_pressed_without_focus_is_not_recorded() {
        let mut focus = KeyboardFocus::default();
        focus.set_pressed_key(Some(Key::Space));
        focus.move_to(Some(NodeId(1)));
        assert!(focus.pressed_key().is_none());
    }
}
