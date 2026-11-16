// SPDX-License-Identifier: GPL-3.0-only
use crate::ui::NodeId;
use crate::view::View;
use crate::view::component::index_of;
use std::ops::Range;

#[derive(Clone)]
pub struct TextPasteTarget {
    id: NodeId,
    value: String,
    range: Range<usize>,
}

impl View {
    #[must_use]
    pub fn text_paste_target(&self, id: NodeId) -> Option<TextPasteTarget> {
        let index = index_of(&self.nodes, id)?;
        if !self.writable_text_target(index) {
            return None;
        }
        let value = self.text_value(index)?;
        let range = self.editing.edits.get(&id)?.selection();
        if !self
            .editing
            .positions
            .get(&id)?
            .shaped_range(&value, &range)
        {
            return None;
        }
        Some(TextPasteTarget {
            id,
            value: value.to_string(),
            range,
        })
    }

    pub fn paste_text_at(&mut self, target: &TextPasteTarget, text: &str) -> bool {
        let Some(index) = self.validated_text_target(target.id, &target.value, &target.range)
        else {
            return false;
        };
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
