// SPDX-License-Identifier: GPL-3.0-only

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

#[derive(Default)]
pub(crate) struct Collected {
    pub samples: Vec<GpuSample>,
    pub failures: Vec<Error>,
}

pub(crate) struct SampleToken {
    timer: RunningTimer,
    tag: SampleTag,
}

pub struct GpuProfiler {
    idle: Vec<GpuTimer>,
    stopped: Vec<(StoppedTimer, SampleTag)>,
    device: Arc<Device>,
}

impl GpuProfiler {
    pub(super) fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let timer = GpuTimer::new(device).map_err(Error::CreateTimer)?;
        Ok(Self {
            idle: vec![timer],
            stopped: Vec::new(),
            device: device.clone(),
        })
    }

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
