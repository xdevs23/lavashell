// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::vulkan::{Collected, GpuProfiler, GpuSample, SampleKind, SampleTag, SampleToken};
use crate::{Error, Painter, SurfaceFrame};
use lavashell_core::frame::GpuFrameTime;
use lavashell_core::surface::{PerSurface, SurfaceId};
use skia_safe::Canvas;
use skia_safe::gpu::DirectContext;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTiming {
    Measured,
    Off,
}

pub(crate) struct GpuTiming {
    profiler: GpuProfiler,
    frame_timing: FrameTiming,
    frames: BTreeMap<SurfaceId, GpuFrameTime>,
}

pub(crate) struct RunningSample(Result<SampleToken, Error>);

struct FrameSample {
    context: DirectContext,
    sample: RunningSample,
}

impl GpuTiming {
    fn new(profiler: GpuProfiler, frame_timing: FrameTiming) -> Self {
        Self {
            profiler,
            frame_timing,
            frames: BTreeMap::new(),
        }
    }

    pub(crate) fn profiler(&mut self) -> &mut GpuProfiler {
        &mut self.profiler
    }

    pub(crate) fn collect(&mut self) -> Collected {
        let Collected { samples, failures } = self.profiler.collect();
        Collected {
            samples: keep_frames(samples, &mut self.frames),
            failures,
        }
    }

    fn begin_frame(&mut self, canvas: &Canvas, tag: SampleTag) -> Option<FrameSample> {
        if self.frame_timing == FrameTiming::Off {
            return None;
        }
        let mut context = canvas.direct_context()?;
        let sample = RunningSample::begin(&mut context, &mut self.profiler, tag);
        Some(FrameSample { context, sample })
    }

    fn end_frame(&mut self, frame: FrameSample) -> Result<(), Error> {
        let FrameSample {
            mut context,
            sample,
        } = frame;
        sample.end(&mut context, &mut self.profiler)
    }
}

impl RunningSample {
    pub(crate) fn begin(
        context: &mut DirectContext,
        profiler: &mut GpuProfiler,
        tag: SampleTag,
    ) -> Self {
        flush_and_submit(context);
        Self(profiler.begin(tag))
    }

    pub(crate) fn end(
        self,
        context: &mut DirectContext,
        profiler: &mut GpuProfiler,
    ) -> Result<(), Error> {
        let token = self.0?;
        flush_and_submit(context);
        profiler.end(token)
    }
}

fn flush_and_submit(context: &mut DirectContext) {
    let _context_returned_for_chaining = context.flush_and_submit();
}

impl PerSurface for GpuTiming {
    fn forget_surface(&mut self, surface: SurfaceId) {
        let _forgotten_time = self.frames.remove(&surface);
    }
}

fn keep_frames(
    samples: Vec<GpuSample>,
    frames: &mut BTreeMap<SurfaceId, GpuFrameTime>,
) -> Vec<GpuSample> {
    let (frame_samples, other_samples) =
        (samples.into_iter()).partition::<Vec<_>, _>(|sample| sample.tag.kind == SampleKind::Frame);
    for sample in frame_samples {
        let time = GpuFrameTime {
            sequence: sample.tag.frame,
            elapsed: sample.elapsed,
        };
        let latest = frames.entry(sample.tag.surface).or_insert(time);
        if latest.sequence < time.sequence {
            *latest = time;
        }
    }
    other_samples
}

impl Painter {
    pub fn time_gpu(&mut self, profiler: GpuProfiler, frame_timing: FrameTiming) {
        self.gpu_timing = Some(GpuTiming::new(profiler, frame_timing));
    }

    pub fn take_gpu_frame_time(&mut self, surface: SurfaceId) -> Option<GpuFrameTime> {
        self.gpu_timing.as_mut()?.frames.remove(&surface)
    }

    pub(crate) fn paint_timed_surface(
        &mut self,
        canvas: &Canvas,
        frame: SurfaceFrame<'_>,
        sequence: u64,
    ) -> Result<(), Error> {
        let tag = SampleTag::frame(frame.surface, sequence);
        let sample = (self.gpu_timing.as_mut()).and_then(|timing| timing.begin_frame(canvas, tag));
        let painted = self.paint_surface(canvas, frame);
        if let Some((timing, sample)) = self.gpu_timing.as_mut().zip(sample)
            && let Err(cause) = timing.end_frame(sample)
        {
            (self.failures).record_caused(DrawFailure::GpuTimestamp, &cause);
        }
        painted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use crate::{Fonts, VulkanRenderer};
    use lavashell_core::scene::tree::SceneTree;
    use lavashell_effect_quality::{DrawId, DrawTimeKey, EffectId, VariantId};
    use std::time::Duration;

    fn frame(surface: u64, sequence: u64) -> GpuSample {
        GpuSample {
            tag: SampleTag::frame(SurfaceId::numbered(surface), sequence),
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
            surface: SurfaceId::numbered(1),
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
                ..SampleTag::frame(SurfaceId::numbered(1), 4)
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
            BTreeMap::from([
                (SurfaceId::numbered(1), time(3)),
                (SurfaceId::numbered(2), time(1))
            ])
        );
        assert!(keep_frames(vec![frame(1, 1)], &mut frames).is_empty());
        assert_eq!(frames[&SurfaceId::numbered(1)], time(3));
    }

    #[test]
    fn a_taken_or_forgotten_surface_has_no_frame_time() -> Result<(), TestError> {
        let gpu = VulkanRenderer::headless()?;
        let mut timing = GpuTiming::new(gpu.create_profiler()?, FrameTiming::Off);
        assert!(keep_frames(vec![frame(1, 1), frame(2, 1)], &mut timing.frames).is_empty());
        timing.forget_surface(SurfaceId::numbered(1));
        assert_eq!(
            timing.frames.keys().collect::<Vec<_>>(),
            [&SurfaceId::numbered(2)]
        );
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        painter.gpu_timing = Some(timing);
        assert_eq!(
            painter.take_gpu_frame_time(SurfaceId::numbered(2)),
            Some(time(1))
        );
        assert_eq!(painter.take_gpu_frame_time(SurfaceId::numbered(2)), None);
        Ok(())
    }
}
