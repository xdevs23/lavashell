// SPDX-License-Identifier: GPL-3.0-only
use crate::Painter;
use crate::vulkan::{Collected, GpuProfiler, GpuSample, SampleKind};
use lavashell_core::surface::SurfaceId;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuFrameTime {
    pub sequence: u64,
    pub elapsed: Duration,
}

#[derive(Default)]
pub(crate) struct GpuTiming {
    profiler: Option<Rc<RefCell<GpuProfiler>>>,
    frames: BTreeMap<SurfaceId, GpuFrameTime>,
}

impl GpuTiming {
    pub(crate) fn profiler(&self) -> Option<&Rc<RefCell<GpuProfiler>>> {
        self.profiler.as_ref()
    }

    /// Stores the completed frame samples and returns the other completed samples with the failures of the timers.
    pub(crate) fn collect(&mut self) -> Collected {
        let Some(profiler) = &self.profiler else {
            return Collected::default();
        };
        let Collected { samples, failures } = profiler.borrow_mut().collect();
        Collected {
            samples: keep_frames(samples, &mut self.frames),
            failures,
        }
    }

    pub(crate) fn forget(&mut self, surface: SurfaceId) {
        let _forgotten_time = self.frames.remove(&surface);
    }
}

/// Stores the newest frame time of each surface in `frames` and returns the samples that are not frame samples.
fn keep_frames(
    mut samples: Vec<GpuSample>,
    frames: &mut BTreeMap<SurfaceId, GpuFrameTime>,
) -> Vec<GpuSample> {
    samples.retain(|sample| {
        if sample.tag.kind != SampleKind::Frame {
            return true;
        }
        let time = GpuFrameTime {
            sequence: sample.tag.frame,
            elapsed: sample.elapsed,
        };
        let latest = frames.entry(sample.tag.surface).or_insert(time);
        if latest.sequence < time.sequence {
            *latest = time;
        }
        false
    });
    samples
}

impl Painter {
    /// Measures the GPU time of effects and frames with `profiler`. [`Painter::take_gpu_frame_time`] returns the
    /// measured frame times.
    pub fn time_gpu(&mut self, profiler: Rc<RefCell<GpuProfiler>>) {
        self.gpu_timing.profiler = Some(profiler);
    }

    /// The latest frame time of `surface` collected since the last call.
    pub fn take_gpu_frame_time(&mut self, surface: SurfaceId) -> Option<GpuFrameTime> {
        self.gpu_timing.frames.remove(&surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fonts;
    use crate::test_error::TestError;
    use crate::vulkan::SampleTag;
    use lavashell_core::scene::tree::SceneTree;
    use lavashell_effect_quality::{DrawId, DrawTimeKey, EffectId, VariantId};

    fn frame(surface: u64, sequence: u64) -> GpuSample {
        GpuSample {
            tag: SampleTag::frame(SurfaceId(surface), sequence),
            elapsed: Duration::from_micros(sequence),
        }
    }

    fn time(sequence: u64) -> GpuFrameTime {
        GpuFrameTime {
            sequence,
            elapsed: Duration::from_micros(sequence),
        }
    }

    #[test]
    fn the_newest_frame_of_each_surface_is_kept_and_effect_samples_are_returned() {
        let draw = DrawId {
            surface: SurfaceId(1),
            leaf: SceneTree::LEAF,
            ordinal: 0,
            effect: EffectId(9),
        };
        let kind = SampleKind::Effect(DrawTimeKey {
            draw,
            variant: VariantId(1),
        });
        let effect = GpuSample {
            tag: SampleTag {
                kind,
                ..SampleTag::frame(SurfaceId(1), 4)
            },
            elapsed: Duration::from_micros(1),
        };
        let mut frames = BTreeMap::new();
        let returned = keep_frames(
            vec![frame(1, 3), effect, frame(1, 2), frame(2, 1)],
            &mut frames,
        );
        let returned: Vec<_> = returned.iter().map(|sample| sample.tag.kind).collect();
        assert_eq!(returned, [kind]);
        assert_eq!(
            frames,
            BTreeMap::from([(SurfaceId(1), time(3)), (SurfaceId(2), time(1))])
        );
        assert!(keep_frames(vec![frame(1, 1)], &mut frames).is_empty());
        assert_eq!(frames[&SurfaceId(1)], time(3));
    }

    #[test]
    fn a_taken_or_forgotten_surface_has_no_frame_time() -> Result<(), TestError> {
        let mut timing = GpuTiming::default();
        assert!(keep_frames(vec![frame(1, 1), frame(2, 1)], &mut timing.frames).is_empty());
        timing.forget(SurfaceId(1));
        assert_eq!(timing.frames.keys().collect::<Vec<_>>(), [&SurfaceId(2)]);
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        painter.gpu_timing = timing;
        assert_eq!(painter.take_gpu_frame_time(SurfaceId(2)), Some(time(1)));
        assert_eq!(painter.take_gpu_frame_time(SurfaceId(2)), None);
        Ok(())
    }
}
