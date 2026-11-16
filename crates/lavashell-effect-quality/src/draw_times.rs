// SPDX-License-Identifier: GPL-3.0-only
use super::{Choice, DrawId, FramePlan, VariantId};
use lavashell_core::surface::{PerSurface, SurfaceId};
use std::{cmp::Reverse, collections::BTreeMap, time::Duration};

const SAMPLES_IN_MEDIAN: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DrawTimeKey {
    pub draw: DrawId,
    pub variant: VariantId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawnTimes {
    samples: u64,
    window: [Option<Duration>; SAMPLES_IN_MEDIAN],
}

impl DrawnTimes {
    fn record(&mut self, gpu_time: Duration) {
        self.window.rotate_right(1);
        self.window[0] = Some(gpu_time);
        self.samples += 1;
    }

    #[must_use]
    pub fn samples(&self) -> u64 {
        self.samples
    }

    #[must_use]
    pub fn expected(&self) -> Option<Duration> {
        let mut descending = self.window;
        descending.sort_unstable_by_key(|sample| Reverse(*sample));
        descending.get(SAMPLES_IN_MEDIAN / 2).copied().flatten()
    }

    #[must_use]
    pub fn jitter(&self) -> Option<Duration> {
        let expected = self.expected()?;
        let smallest = self.window.iter().flatten().min()?;
        Some(expected.saturating_sub(*smallest))
    }

    pub(crate) fn window_renewed_since(&self, samples: u64) -> bool {
        (samples..self.samples).nth(SAMPLES_IN_MEDIAN - 1).is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuTime {
    pub estimate: Option<Duration>,
    pub drawn: DrawnTimes,
}

impl GpuTime {
    #[must_use]
    pub fn expected(&self) -> Option<Duration> {
        self.drawn.expected().or(self.estimate)
    }
}

#[derive(Clone, Copy, Default)]
enum Estimate {
    #[default]
    Unmeasured,
    Pending,
    Unmeasurable,
    Known(Duration),
}

#[derive(Default)]
struct VariantTimes {
    estimate: Estimate,
    drawn: DrawnTimes,
}

#[derive(Default)]
pub struct DrawTimes {
    variants: BTreeMap<DrawTimeKey, VariantTimes>,
}

impl DrawTimes {
    fn entry(&mut self, key: DrawTimeKey) -> &mut VariantTimes {
        self.variants.entry(key).or_default()
    }

    pub fn record(&mut self, key: DrawTimeKey, gpu_time: Duration) {
        self.entry(key).drawn.record(gpu_time);
    }

    #[must_use]
    pub fn needs_first_measurement(&self, key: DrawTimeKey) -> bool {
        let estimate = self.variants.get(&key).map(|times| times.estimate);
        matches!(estimate, None | Some(Estimate::Unmeasured))
    }

    pub fn await_first_measurement(&mut self, key: DrawTimeKey) {
        self.entry(key).estimate = Estimate::Pending;
    }

    pub fn mark_unmeasurable(&mut self, key: DrawTimeKey) {
        self.entry(key).estimate = Estimate::Unmeasurable;
    }

    pub fn record_first_measurement(&mut self, key: DrawTimeKey, gpu_time: Duration) {
        let times = self.variants.get_mut(&key);
        if let Some(times) = times.filter(|times| matches!(times.estimate, Estimate::Pending)) {
            times.estimate = Estimate::Known(gpu_time);
        }
    }

    #[must_use]
    pub fn gpu_time(&self, key: DrawTimeKey) -> GpuTime {
        let Some(times) = self.variants.get(&key) else {
            return GpuTime::default();
        };
        GpuTime {
            estimate: match times.estimate {
                Estimate::Known(gpu_time) => Some(gpu_time),
                Estimate::Unmeasured | Estimate::Pending | Estimate::Unmeasurable => None,
            },
            drawn: times.drawn,
        }
    }

    pub fn retain_chosen(&mut self, surface: SurfaceId, plan: &FramePlan) {
        self.variants.retain(|key, times| {
            if key.draw.surface != surface {
                return true;
            }
            let choice = plan.choice(key.draw);
            if choice != Some(Choice::Draw(key.variant)) {
                if let Some(expected) = times.drawn.expected() {
                    times.estimate = Estimate::Known(expected);
                }
                times.drawn = DrawnTimes::default();
            }
            choice.is_some()
        });
    }
}

impl PerSurface for DrawTimes {
    fn forget_surface(&mut self, surface: SurfaceId) {
        self.variants.retain(|key, _| key.draw.surface != surface);
    }
}

#[cfg(test)]
mod tests;
