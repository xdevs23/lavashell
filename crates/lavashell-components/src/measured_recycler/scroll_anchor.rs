// SPDX-License-Identifier: GPL-3.0-only
use super::measured_rows::MeasuredRows;
use lavashell_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScrollMode {
    KeepPosition,
    FollowEnd,
}

struct PendingScroll {
    offset: Dp,
    mode: ScrollMode,
}

#[derive(Default)]
pub(super) struct ScrollAnchor {
    slot: Option<usize>,
    observed_offset: Dp,
    pending: Option<PendingScroll>,
}

impl ScrollAnchor {
    pub(super) fn top<K>(&self, rows: &MeasuredRows<K>) -> Dp {
        self.slot.map_or(Dp::ZERO, |slot| rows.prefix(slot))
    }

    pub(super) fn follows(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.mode == ScrollMode::FollowEnd)
    }

    pub(super) fn reset(&mut self) {
        self.slot = None;
        self.pending = None;
    }

    pub(super) fn follow_end(&mut self, offset: Dp) {
        self.pending = Some(PendingScroll {
            offset,
            mode: ScrollMode::FollowEnd,
        });
    }

    pub(super) fn shift(&mut self, shift: Dp, current: impl FnOnce() -> Dp) {
        if shift != Dp::ZERO {
            let offset = (self.pending.as_ref()).map_or_else(current, |pending| pending.offset);
            self.pending = Some(PendingScroll {
                offset: offset + shift,
                mode: ScrollMode::KeepPosition,
            });
        }
    }

    pub(super) fn reanchor(&mut self, slot: usize, offset: Dp) {
        self.pending = None;
        self.slot = Some(slot);
        self.observed_offset = offset;
    }

    pub(super) fn observe<K>(&mut self, rows: &MeasuredRows<K>, offset: Dp) {
        if self.slot.is_none() || !rows.same_length(self.observed_offset, offset) {
            self.slot = Some(rows.find(offset));
            self.observed_offset = offset;
        }
    }

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
