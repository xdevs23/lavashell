// SPDX-License-Identifier: GPL-3.0-only
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationPolicy {
    Automatic,
    Explicit,
}

/// Whether one native Surface has presented a frame with an explicit presentation time. Every acquisition of the
/// Surface and every clone of this value share the same policy.
/// After one frame with an explicit time, every later frame of the Surface needs one too, also after the Surface is
/// detached and attached again.
/// `Automatic` is valid only while the Surface has never presented with an explicit time. A Surface with an unknown
/// history therefore starts with `Explicit`.
#[derive(Clone)]
pub struct SurfaceTiming(Rc<Cell<PresentationPolicy>>);

impl SurfaceTiming {
    #[must_use]
    pub fn new(policy: PresentationPolicy) -> Self {
        Self(Rc::new(Cell::new(policy)))
    }

    #[must_use]
    pub fn policy(&self) -> PresentationPolicy {
        self.0.get()
    }

    pub(super) fn check(&self, explicit: bool) -> Result<(), crate::Error> {
        if self.policy() == PresentationPolicy::Explicit && !explicit {
            return Err(crate::Error::ExplicitTimestampRequired);
        }
        Ok(())
    }

    pub(super) fn submitting(&self, explicit: bool) {
        if explicit {
            self.0.set(PresentationPolicy::Explicit);
        }
    }
}
