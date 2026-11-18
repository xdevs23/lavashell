// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::{DrawFailure, Failures};
use crate::{Error, Painter, SurfaceFrame};
use lavashell_core::frame::GpuFrameTime;
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_effect_quality::DrawTimeKey;
use skia_safe::Canvas;
use skia_safe::gpu::DirectContext;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTiming {
    Measured,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    Frame,
    Effect(DrawTimeKey),
    FirstEffectDraw(DrawTimeKey),
}

#[derive(Clone, Copy, Debug)]
pub struct SampleTag {
    pub kind: SampleKind,
    pub surface: SurfaceId,
    pub frame: u64,
}

impl SampleTag {
    #[must_use]
    pub fn frame(surface: SurfaceId, frame: u64) -> Self {
        Self {
            kind: SampleKind::Frame,
            surface,
            frame,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GpuSample {
    pub tag: SampleTag,
    pub elapsed: Duration,
}

pub struct Collected<E> {
    pub samples: Vec<GpuSample>,
    pub failures: Vec<E>,
}

impl<E> Default for Collected<E> {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            failures: Vec::new(),
        }
    }
}

pub trait GpuSampleTimer {
    type Running;
    type Error: std::error::Error + 'static;

    fn begin(&mut self, tag: SampleTag) -> Result<Self::Running, Self::Error>;

    fn end(&mut self, running: Self::Running) -> Result<(), Self::Error>;

    fn collect(&mut self) -> Collected<Self::Error>;
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampleOutcome {
    Recorded,
    Lost,
}

pub(crate) struct GpuTiming {
    timer: Box<dyn RecordedSamples>,
    frame_timing: FrameTiming,
    frames: BTreeMap<SurfaceId, GpuFrameTime>,
}

#[must_use]
struct FrameTicket {
    context: DirectContext,
}

trait RecordedSamples {
    fn begin_frame(&mut self, tag: SampleTag);

    fn end_frame(&mut self, failures: &Failures);

    fn time(
        &mut self,
        tag: SampleTag,
        draw: &mut dyn FnMut(),
        failures: &Failures,
    ) -> SampleOutcome;

    fn collect(&mut self, failures: &Failures) -> Vec<GpuSample>;
}

struct RecordingTimer<T: GpuSampleTimer> {
    timer: T,
    frame: Option<Result<T::Running, T::Error>>,
}

impl GpuTiming {
    fn new(timer: impl GpuSampleTimer + 'static, frame_timing: FrameTiming) -> Self {
        Self {
            timer: Box::new(RecordingTimer { timer, frame: None }),
            frame_timing,
            frames: BTreeMap::new(),
        }
    }

    pub(crate) fn collect(&mut self, failures: &Failures) -> Vec<GpuSample> {
        keep_frames(self.timer.collect(failures), &mut self.frames)
    }

    pub(crate) fn time(
        &mut self,
        context: &mut DirectContext,
        tag: SampleTag,
        draw: &mut dyn FnMut(),
        failures: &Failures,
    ) -> SampleOutcome {
        flush_and_submit(context);
        let mut draw_and_submit = || {
            draw();
            flush_and_submit(context);
        };
        self.timer.time(tag, &mut draw_and_submit, failures)
    }

    fn begin_frame(&mut self, canvas: &Canvas, tag: SampleTag) -> Option<FrameTicket> {
        if self.frame_timing == FrameTiming::Off {
            return None;
        }
        let mut context = canvas.direct_context()?;
        flush_and_submit(&mut context);
        self.timer.begin_frame(tag);
        Some(FrameTicket { context })
    }

    fn end_frame(&mut self, ticket: FrameTicket, failures: &Failures) {
        let FrameTicket { mut context } = ticket;
        flush_and_submit(&mut context);
        self.timer.end_frame(failures);
    }
}

impl<T: GpuSampleTimer> RecordedSamples for RecordingTimer<T> {
    fn begin_frame(&mut self, tag: SampleTag) {
        self.frame = Some(self.timer.begin(tag));
    }

    fn end_frame(&mut self, failures: &Failures) {
        if let Some(begun) = self.frame.take()
            && let Err(cause) = self.end(begun)
        {
            record_timing_failure(failures, &cause);
        }
    }

    fn time(
        &mut self,
        tag: SampleTag,
        draw: &mut dyn FnMut(),
        failures: &Failures,
    ) -> SampleOutcome {
        let begun = self.timer.begin(tag);
        draw();
        match self.end(begun) {
            Ok(()) => SampleOutcome::Recorded,
            Err(cause) => {
                record_timing_failure(failures, &cause);
                SampleOutcome::Lost
            }
        }
    }

    fn collect(&mut self, failures: &Failures) -> Vec<GpuSample> {
        let collected = self.timer.collect();
        for cause in &collected.failures {
            record_timing_failure(failures, cause);
        }
        collected.samples
    }
}

impl<T: GpuSampleTimer> RecordingTimer<T> {
    fn end(&mut self, begun: Result<T::Running, T::Error>) -> Result<(), T::Error> {
        begun.and_then(|running| self.timer.end(running))
    }
}

fn record_timing_failure(failures: &Failures, cause: &(dyn std::error::Error + 'static)) {
    failures.record_caused(DrawFailure::GpuTimestamp, cause);
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
    pub fn time_gpu(&mut self, timer: impl GpuSampleTimer + 'static, frame_timing: FrameTiming) {
        self.gpu_timing = Some(GpuTiming::new(timer, frame_timing));
    }

    pub fn take_gpu_frame_time(&mut self, surface: SurfaceId) -> Option<GpuFrameTime> {
        self.gpu_timing.as_mut()?.frames.remove(&surface)
    }

    pub fn paint_timed_surface(
        &mut self,
        canvas: &Canvas,
        frame: SurfaceFrame<'_>,
        sequence: u64,
    ) -> Result<(), Error> {
        let tag = SampleTag::frame(frame.surface, sequence);
        let ticket = (self.gpu_timing.as_mut()).and_then(|timing| timing.begin_frame(canvas, tag));
        let painted = self.paint_surface(canvas, frame);
        if let Some((timing, ticket)) = self.gpu_timing.as_mut().zip(ticket) {
            timing.end_frame(ticket, &self.failures);
        }
        painted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fonts;
    use crate::test_error::TestError;
    use lavashell_core::scene::tree::SceneTree;
    use lavashell_effect_quality::{DrawId, EffectId, VariantId};
    use std::convert::Infallible;

    struct IdleTimer;

    impl GpuSampleTimer for IdleTimer {
        type Running = ();
        type Error = Infallible;

        fn begin(&mut self, _: SampleTag) -> Result<(), Infallible> {
            Ok(())
        }

        fn end(&mut self, (): ()) -> Result<(), Infallible> {
            Ok(())
        }

        fn collect(&mut self) -> Collected<Infallible> {
            Collected::default()
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("the test timer fails")]
    struct TimerFault;

    struct FailingTimer {
        begin_fails: bool,
    }

    impl GpuSampleTimer for FailingTimer {
        type Running = ();
        type Error = TimerFault;

        fn begin(&mut self, _: SampleTag) -> Result<(), TimerFault> {
            if std::mem::replace(&mut self.begin_fails, false) {
                Err(TimerFault)
            } else {
                Ok(())
            }
        }

        fn end(&mut self, (): ()) -> Result<(), TimerFault> {
            Err(TimerFault)
        }

        fn collect(&mut self) -> Collected<TimerFault> {
            Collected {
                samples: vec![frame(1, 1)],
                failures: vec![TimerFault],
            }
        }
    }

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
    fn every_failure_of_the_timer_is_recorded_once_and_a_failed_sample_is_lost() {
        let timer = FailingTimer { begin_fails: true };
        let mut timing = GpuTiming::new(timer, FrameTiming::Measured);
        let recorded = Failures::default();
        let tag = SampleTag::frame(SurfaceId::numbered(1), 1);
        timing.timer.begin_frame(tag);
        timing.timer.end_frame(&recorded);
        let mut draws = 0;
        let outcome = timing.timer.time(tag, &mut || draws += 1, &recorded);
        let samples = timing.collect(&recorded);
        assert_eq!(draws, 1);
        assert_eq!(outcome, SampleOutcome::Lost);
        assert!(samples.is_empty(), "{samples:?}");
        assert_eq!(timing.frames[&SurfaceId::numbered(1)], time(1));
        assert_eq!(recorded.count(DrawFailure::GpuTimestamp), 3);
    }

    #[test]
    fn a_taken_or_forgotten_surface_has_no_frame_time() -> Result<(), TestError> {
        let mut timing = GpuTiming::new(IdleTimer, FrameTiming::Off);
        assert!(keep_frames(vec![frame(1, 1), frame(2, 1)], &mut timing.frames).is_empty());
        timing.forget_surface(SurfaceId::numbered(1));
        assert_eq!(
            timing.frames.keys().collect::<Vec<_>>(),
            [&SurfaceId::numbered(2)]
        );
        let mut painter = Painter::without_device(&Fonts::new())?;
        painter.gpu_timing = Some(timing);
        assert_eq!(
            painter.take_gpu_frame_time(SurfaceId::numbered(2)),
            Some(time(1))
        );
        assert_eq!(painter.take_gpu_frame_time(SurfaceId::numbered(2)), None);
        Ok(())
    }
}
