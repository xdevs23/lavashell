// SPDX-License-Identifier: GPL-3.0-only
use super::{Choice, DrawId, FramePlan, VariantId};
use lavashell_core::surface::SurfaceId;
use std::{cmp::Reverse, collections::BTreeMap, time::Duration};

/// The number of newest drawn samples from which the expected time and the jitter of a variant are computed. Three
/// samples give the largest, which is ignored as a possible outlier, the second largest, which is the expected time,
/// and the smallest, which sets the jitter.
const WINDOW: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DrawTimeKey {
    pub draw: DrawId,
    pub variant: VariantId,
}

/// The GPU times measured for the newest draws of one effect draw with one variant, counted since the variant was
/// chosen for that draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawnTimes {
    samples: u64,
    /// The newest sample first.
    window: [Option<Duration>; WINDOW],
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

    /// The second largest sample in the window. A single slow draw does not change it, but a slow draw that happens
    /// twice within the window does. `None` while the window has fewer than two samples.
    #[must_use]
    pub fn expected(&self) -> Option<Duration> {
        let mut descending = self.window;
        descending.sort_unstable_by_key(|sample| Reverse(*sample));
        descending.get(1).copied().flatten()
    }

    /// How far the samples of the window spread below the expected time.
    #[must_use]
    pub fn jitter(&self) -> Option<Duration> {
        let expected = self.expected()?;
        let smallest = self.window.iter().flatten().min()?;
        Some(expected.saturating_sub(*smallest))
    }

    /// Whether every sample of the window was recorded after the first `samples` samples.
    pub(crate) fn window_renewed_since(&self, samples: u64) -> bool {
        (samples..self.samples).nth(WINDOW - 1).is_some()
    }
}

/// The measured GPU times of one variant for one effect draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuTime {
    /// Measured once, when the draw first appears, by drawing the variant on a blank copy of the target canvas.
    /// Replaced by the expected time of the drawn samples whenever the variant stops being drawn.
    pub estimate: Option<Duration>,
    pub drawn: DrawnTimes,
}

impl GpuTime {
    /// The expected time of the drawn samples once the window has two samples, otherwise the estimate. `None` while
    /// neither exists.
    #[must_use]
    pub fn expected(&self) -> Option<Duration> {
        self.drawn.expected().or(self.estimate)
    }
}

#[derive(Clone, Copy, Default)]
enum Estimate {
    #[default]
    Unmeasured,
    /// The first measurement was submitted to the GPU, and its result has not arrived yet.
    Pending,
    /// The first measurement could not be submitted. The time of the variant comes only from its drawn samples.
    Unmeasurable,
    Known(Duration),
}

#[derive(Default)]
struct VariantTimes {
    estimate: Estimate,
    drawn: DrawnTimes,
}

/// The GPU times of effect draws, per surface, effect draw and variant.
#[derive(Default)]
pub struct DrawTimes {
    surfaces: BTreeMap<SurfaceId, BTreeMap<DrawTimeKey, VariantTimes>>,
}

impl DrawTimes {
    fn entry(&mut self, surface: SurfaceId, key: DrawTimeKey) -> &mut VariantTimes {
        self.surfaces
            .entry(surface)
            .or_default()
            .entry(key)
            .or_default()
    }

    /// Records `gpu_time` as a drawn sample of `key` on `surface`. A sample arrives after the GPU finishes the draw.
    /// The timing of a draw flushes the GPU work before and after it, so a sample measures that draw alone.
    pub fn record(&mut self, surface: SurfaceId, key: DrawTimeKey, gpu_time: Duration) {
        self.entry(surface, key).drawn.record(gpu_time);
    }

    #[must_use]
    pub fn needs_first_measurement(&self, surface: SurfaceId, key: DrawTimeKey) -> bool {
        let estimate = (self.surfaces.get(&surface))
            .and_then(|draws| draws.get(&key))
            .map(|times| times.estimate);
        matches!(estimate, None | Some(Estimate::Unmeasured))
    }

    /// Marks the first measurement as submitted, so that it is not submitted again before it arrives.
    pub fn await_first_measurement(&mut self, surface: SurfaceId, key: DrawTimeKey) {
        self.entry(surface, key).estimate = Estimate::Pending;
    }

    /// Marks a first measurement that could not be submitted, so that it is not tried again.
    pub fn mark_unmeasurable(&mut self, surface: SurfaceId, key: DrawTimeKey) {
        self.entry(surface, key).estimate = Estimate::Unmeasurable;
    }

    /// Records `gpu_time` as the estimate of `key` on `surface` while the first measurement is pending. The measurement
    /// is dropped when the draw was removed before the measurement arrived, or when the estimate already came from
    /// drawn samples.
    pub fn record_first_measurement(
        &mut self,
        surface: SurfaceId,
        key: DrawTimeKey,
        gpu_time: Duration,
    ) {
        let times = (self.surfaces.get_mut(&surface)).and_then(|draws| draws.get_mut(&key));
        if let Some(times) = times.filter(|times| matches!(times.estimate, Estimate::Pending)) {
            times.estimate = Estimate::Known(gpu_time);
        }
    }

    #[must_use]
    pub fn gpu_time(&self, surface: SurfaceId, key: DrawTimeKey) -> GpuTime {
        let Some(times) = (self.surfaces.get(&surface)).and_then(|draws| draws.get(&key)) else {
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

    /// Removes the times of every draw that `plan` does not contain. For every variant that `plan` does not draw, the
    /// drawn samples start over, because the window of a variant begins when the variant is chosen. The expected time
    /// of the old samples becomes the estimate of that variant. A variant that was lowered because its real draws were
    /// too slow is then not raised again because of a faster first measurement on a blank copy.
    pub fn retain_chosen(&mut self, surface: SurfaceId, plan: &FramePlan) {
        let Some(draws) = self.surfaces.get_mut(&surface) else {
            return;
        };
        draws.retain(|key, times| {
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

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.surfaces.retain(|kept, _| *kept != surface);
    }
}

#[cfg(test)]
mod tests;
