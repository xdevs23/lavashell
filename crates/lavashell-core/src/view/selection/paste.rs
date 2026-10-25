// SPDX-License-Identifier: GPL-3.0-only
use super::{NodeId, View};
use std::ops::Range;

/// An editable text field, with the text and the selection shown by the field when the host requested the content of
/// the clipboard. The content arrives later and replaces that selection.
#[derive(Clone)]
pub struct TextPasteTarget {
    id: NodeId,
    value: String,
    range: Range<usize>,
}

impl View {
    #[must_use]
    pub fn text_paste_target(&self, id: NodeId) -> Option<TextPasteTarget> {
        let index = self.nodes.iter().position(|node| node.id == id)?;
        if !self.writable_text_target(index) {
            return None;
        }
        let value = self.text_value(index)?;
        let range = self.edits.get(&id)?.selection();
        if !self.text_positions.get(&id)?.shaped_range(&value, &range) {
            return None;
        }
        Some(TextPasteTarget {
            id,
            value: value.to_string(),
            range,
        })
    }

    /// Replaces the selection recorded in `target` with `text`, whatever node has the focus and whatever is selected
    /// now. Returns false and changes nothing when the field no longer shows the recorded text or can no longer be
    /// edited.
    pub fn paste_text_at(&mut self, target: &TextPasteTarget, text: &str) -> bool {
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
        let Some(index) = self.nodes.iter().position(|node| node.id == target.id) else {
            return false;
        };
        if !self.writable_text_target(index)
            || self.text_value(index).as_deref() != Some(target.value.as_str())
            || !(self.text_positions.get(&target.id))
                .is_some_and(|p| p.shaped_range(&target.value, &target.range))
        {
            return false;
        }
        let text = self.normalize_for_field(index, text);
        self.edit_at(index, |edit, value| {
            edit.cancel_drag();
            edit.cursor = target.range.end;
            edit.anchor = Some(target.range.start);
            edit.replace(value, &text);
        });
        true
    }
}
