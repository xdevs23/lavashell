// SPDX-License-Identifier: GPL-3.0-only
use super::measured_rows::MeasuredRows;
use lavashell_core::prelude::*;

/// A scroll offset to apply once the content extent of the scroll matches the rows, and whether the
/// offset follows the end of the rows.
struct PendingScroll {
    offset: Dp,
    follow: bool,
}

/// The anchor row of a measured list: the row at the top of the viewport. When the heights of the
/// rows above the anchor row change, the scroll offset moves by the same distance. The anchor row
/// is then at the same position on screen as before.
#[derive(Default)]
pub(super) struct ScrollAnchor {
    slot: Option<usize>,
    /// The scroll offset at the last call of `observe` or `reanchor`.
    observed_offset: Dp,
    pending: Option<PendingScroll>,
}

impl ScrollAnchor {
    /// The top of the anchor row, or zero without an anchor row.
    pub(super) fn top<K>(&self, rows: &MeasuredRows<K>) -> Dp {
        self.slot.map_or(Dp::ZERO, |slot| rows.prefix(slot))
    }

    /// Whether the pending scroll follows the end of the rows.
    pub(super) fn follows(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| pending.follow)
    }

    /// Clears the anchor row and the pending scroll, for rows that got new keys.
    pub(super) fn reset(&mut self) {
        self.slot = None;
        self.pending = None;
    }

    /// Sets `offset` as the pending scroll, which follows the end of the rows. `settle` returns the
    /// offset once the content extent of the scroll matches the rows.
    pub(super) fn follow_end(&mut self, offset: Dp) {
        self.pending = Some(PendingScroll {
            offset,
            follow: true,
        });
    }

    /// Moves the pending scroll by `shift`. Without a pending scroll, starts from the offset returned
    /// by `current`. The moved scroll does not follow the end of the rows.
    pub(super) fn shift(&mut self, shift: Dp, current: impl FnOnce() -> Dp) {
        if shift != Dp::ZERO {
            let offset = (self.pending.as_ref()).map_or_else(current, |pending| pending.offset);
            self.pending = Some(PendingScroll {
                offset: offset + shift,
                follow: false,
            });
        }
    }

    /// Sets the row at `slot` as the anchor row at the scroll offset `offset`, and clears the pending
    /// scroll.
    pub(super) fn reanchor(&mut self, slot: usize, offset: Dp) {
        self.pending = None;
        self.slot = Some(slot);
        self.observed_offset = offset;
    }

    /// Sets the row at the scroll offset `offset` as the anchor row, when there is no anchor row or
    /// when the scroll moved by one device pixel or more since the last check.
    pub(super) fn observe<K>(&mut self, rows: &MeasuredRows<K>, offset: Dp) {
        if self.slot.is_none() || !rows.same_length(self.observed_offset, offset) {
            self.slot = Some(rows.find(offset));
            self.observed_offset = offset;
        }
    }

    /// Returns the pending scroll and clears it and the anchor row, once the content extent
    /// `content` of the scroll matches the height of all rows. The scroll measures its children
    /// before it updates its content extent. An offset applied before that update would be clamped
    /// to the old extent.
    pub(super) fn settle<K>(&mut self, rows: &MeasuredRows<K>, content: Dp) -> Option<Dp> {
        if rows.same_length(content, rows.total())
            && let Some(pending) = self.pending.take()
        {
            self.slot = None;
            Some(pending.offset)
        } else {
            None
        }
    }
}
