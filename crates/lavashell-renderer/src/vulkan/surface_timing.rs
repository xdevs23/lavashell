// SPDX-License-Identifier: GPL-3.0-only
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationPolicy {
    Automatic,
    Explicit,
}

/// Whether one surface has presented a frame with an explicit presentation time. Every clone of this value shares the
/// same policy. After one frame with an explicit time, every later frame of the surface needs one too.
/// `Automatic` is valid only while the surface has never presented with an explicit time.
#[derive(Clone)]
pub struct SurfaceTiming(Arc<AtomicBool>);

impl SurfaceTiming {
    #[must_use]
    pub fn new(policy: PresentationPolicy) -> Self {
        Self(Arc::new(AtomicBool::new(
            policy == PresentationPolicy::Explicit,
        )))
    }

    #[must_use]
    pub fn policy(&self) -> PresentationPolicy {
        if self.0.load(Ordering::Relaxed) {
            PresentationPolicy::Explicit
        } else {
            PresentationPolicy::Automatic
        }
    }

    pub(super) fn check(&self, explicit: bool) -> Result<(), crate::Error> {
        if self.policy() == PresentationPolicy::Explicit && !explicit {
            return Err(crate::Error::ExplicitTimestampRequired);
        }
        Ok(())
    }

    pub(super) fn submitting(&self, explicit: bool) {
        if explicit {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}
