// SPDX-License-Identifier: GPL-3.0-only
use crate::bind::bind_global;
use crate::sctk;
use crate::translator::Translator;
use lavashell_actor::Clock;
use lavashell_core::frame::{FrameTime, PresentationFlag, PresentationFlags};
use lavashell_core::surface::SurfaceId;
use rustix::time::ClockId;
use sctk::reexports::client::QueueHandle;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::protocols::wp::presentation_time::client::{
    wp_presentation::WpPresentation, wp_presentation_feedback,
};
use std::time::Duration;

pub(crate) fn presentation_flags(kind: wp_presentation_feedback::Kind) -> PresentationFlags {
    use wp_presentation_feedback::Kind;
    [
        (Kind::Vsync, PresentationFlag::Vsync),
        (Kind::HwClock, PresentationFlag::HardwareClock),
        (Kind::HwCompletion, PresentationFlag::HardwareCompletion),
        (Kind::ZeroCopy, PresentationFlag::ZeroCopy),
    ]
    .into_iter()
    .filter(|(wire, _)| kind.contains(*wire))
    .map(|(_, flag)| flag)
    .collect()
}

enum PresentationClock {
    Monotonic { epoch: Duration },
    Other(ClockId),
}

impl PresentationClock {
    fn read(clk_id: u32, clock: &dyn Clock) -> Option<Self> {
        let Some(id) = i32::try_from(clk_id)
            .ok()
            .and_then(|id| ClockId::try_from(id).ok())
        else {
            log::warn!("the compositor reports the unknown presentation clock {clk_id}");
            return None;
        };
        match id {
            ClockId::Monotonic => {
                let absolute = Duration::try_from(rustix::time::clock_gettime(id)).ok()?;
                Some(Self::Monotonic {
                    epoch: absolute.checked_sub(clock.now())?,
                })
            }
            other => Some(Self::Other(other)),
        }
    }
}

pub(crate) struct PresentationFeedbackSource {
    proxy: WpPresentation,
    clock: Option<PresentationClock>,
}

impl PresentationFeedbackSource {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Option<Self> {
        Some(Self {
            proxy: bind_global(globals, qh)?,
            clock: None,
        })
    }

    pub(crate) fn proxy(&self) -> &WpPresentation {
        &self.proxy
    }

    pub(crate) fn read_clock(&mut self, clk_id: u32, clock: &dyn Clock) {
        self.clock = PresentationClock::read(clk_id, clock);
    }

    pub(crate) fn observed(&self, absolute: Duration, clock: &dyn Clock) -> Option<Duration> {
        match self.clock.as_ref()? {
            PresentationClock::Monotonic { epoch } => absolute.checked_sub(*epoch),
            PresentationClock::Other(id) => {
                let before = clock.now();
                let clock_now = Duration::try_from(rustix::time::clock_gettime(*id)).ok()?;
                let after = clock.now();
                let age = clock_now.checked_sub(absolute)?;
                midpoint(before, after).checked_sub(age)
            }
        }
    }
}

fn midpoint(earlier: Duration, later: Duration) -> Duration {
    earlier + later.saturating_sub(earlier) / 2
}

pub(crate) struct PresentationFeedbackTarget {
    pub surface: SurfaceId,
    pub timing: FrameTime,
}

#[cfg(test)]
mod tests;
