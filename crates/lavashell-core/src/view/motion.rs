// SPDX-License-Identifier: GPL-3.0-only
use super::{Phase, View};
use crate::preferences::REDUCED_MOTION;

impl View {
    pub(super) fn follow_reduced_motion(&mut self) {
        if self.motion_scope.dirty().any() {
            let reduced = self.root_values.value(&REDUCED_MOTION);
            let reduced = self
                .motion_scope
                .track(Phase::Composition, || reduced.get());
            self.motion.reduced_motion(reduced);
        }
    }
}
