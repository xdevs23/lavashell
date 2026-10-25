// SPDX-License-Identifier: GPL-3.0-only
//! The GPU time of spans of work on the queue, measured with GPU timestamps that are read without waiting. Each
//! sample is the time elapsed between two timestamps and does not measure how busy the GPU was.

use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::DrawTimeKey;
use lavashell_vulkan::{Device, GpuTimer, Reading, RunningTimer, StoppedTimer};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampleKind {
    Frame,
    Effect(DrawTimeKey),
    /// The first measurement of an effect variant without a measured draw time, drawn once on a blank copy of the
    /// canvas in the frame in which a draw of the variant appeared.
    FirstEffectDraw(DrawTimeKey),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SampleTag {
    pub kind: SampleKind,
    pub surface: SurfaceId,
    pub frame: u64,
}

impl SampleTag {
    pub(crate) fn frame(surface: SurfaceId, frame: u64) -> Self {
        Self {
            kind: SampleKind::Frame,
            surface,
            frame,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuSample {
    pub tag: SampleTag,
    pub elapsed: Duration,
}

/// The samples of the timers that finished, and the failures of the timers that could not be read or reset.
#[derive(Default)]
pub(crate) struct Collected {
    pub samples: Vec<GpuSample>,
    pub failures: Vec<Error>,
}

/// A GPU timer that runs from `GpuProfiler::begin` until the token is passed to `GpuProfiler::end`.
pub(crate) struct SampleToken {
    timer: RunningTimer,
    tag: SampleTag,
}

/// Measures the GPU time of spans of work on the queue. It creates a GPU timer for every span that is timed while
/// all other timers are running or have an unread result, and reuses each timer once its result is read.
pub struct GpuProfiler {
    idle: Vec<GpuTimer>,
    stopped: Vec<(StoppedTimer, SampleTag)>,
    device: Arc<Device>,
}

impl GpuProfiler {
    /// # Errors
    /// Fails when the queue of `device` records no timestamps, or the driver cannot create a timer.
    pub(super) fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let timer = GpuTimer::new(device).map_err(Error::CreateTimer)?;
        Ok(Self {
            idle: vec![timer],
            stopped: Vec::new(),
            device: device.clone(),
        })
    }

    /// Starts timing `tag` with an idle timer, or with a new timer when every timer is running or has an unread
    /// result.
    pub(crate) fn begin(&mut self, tag: SampleTag) -> Result<SampleToken, Error> {
        let timer = match self.idle.pop() {
            Some(timer) => timer,
            None => GpuTimer::new(&self.device).map_err(Error::CreateTimer)?,
        };
        Ok(SampleToken {
            timer: timer.start().map_err(Error::StartTimer)?,
            tag,
        })
    }

    pub(crate) fn end(&mut self, token: SampleToken) -> Result<(), Error> {
        let stopped = token.timer.stop().map_err(Error::StopTimer)?;
        self.stopped.push((stopped, token.tag));
        Ok(())
    }

    /// The samples whose timers have finished, without waiting for the others. A timer that cannot be read or reset
    /// is dropped, and its failure is collected with the samples.
    pub(crate) fn collect(&mut self) -> Collected {
        let mut collected = Collected::default();
        let mut waiting = Vec::new();
        for (timer, tag) in std::mem::take(&mut self.stopped) {
            match timer.reading() {
                Ok(Reading::Elapsed(elapsed)) => {
                    collected.samples.push(GpuSample { tag, elapsed });
                    match timer.reset() {
                        Ok(timer) => self.idle.push(timer),
                        Err(error) => collected.failures.push(Error::ResetTimer(error)),
                    }
                }
                Ok(Reading::Pending) => waiting.push((timer, tag)),
                Err(error) => collected.failures.push(Error::ReadTimer(error)),
            }
        }
        self.stopped = waiting;
        collected
    }
}
