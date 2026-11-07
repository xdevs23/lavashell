// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::frame::PresentationFlags;

const WINDOW: SurfaceId = SurfaceId(1);

#[derive(Debug, thiserror::Error)]
#[error("the scheduler did not return the expected output")]
struct MissingOutput;

type TestResult = Result<(), MissingOutput>;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

/// A frame scheduler fed one input at a time, at a time given in milliseconds.
struct Script(FrameScheduler);

impl Script {
    /// A script whose window has opened.
    fn opened(redraw: Redraw) -> Self {
        let mut script = Self(FrameScheduler::new(redraw));
        assert!(script.at(0, SchedulerInput::Opened(WINDOW)).is_empty());
        script
    }

    fn at(&mut self, millis: u64, input: SchedulerInput) -> Vec<SchedulerOutput> {
        let mut out = Vec::new();
        self.0.handle(ms(millis), input, &mut out);
        out
    }

    fn frame_at(
        &mut self,
        millis: u64,
        input: SchedulerInput,
        expected: FrameKind,
    ) -> Result<FrameTime, MissingOutput> {
        match self.at(millis, input).as_slice() {
            [
                SchedulerOutput::Frame {
                    surface: WINDOW,
                    kind,
                    time,
                },
            ] if *kind == expected => Ok(*time),
            _ => Err(MissingOutput),
        }
    }

    fn compose_at(
        &mut self,
        millis: u64,
        input: SchedulerInput,
    ) -> Result<FrameTime, MissingOutput> {
        self.frame_at(millis, input, FrameKind::Compose)
    }

    fn render_at(
        &mut self,
        millis: u64,
        input: SchedulerInput,
    ) -> Result<FrameTime, MissingOutput> {
        self.frame_at(millis, input, FrameKind::Render)
    }

    /// Composes the frame started by `input` at `millis`, and renders it once its scene is built at the same time.
    fn drawn_at(&mut self, millis: u64, input: SchedulerInput) -> Result<FrameTime, MissingOutput> {
        let composed = self.compose_at(millis, input)?;
        let drawn = self.render_at(millis, built(composed.sequence))?;
        assert_eq!(drawn.sequence, composed.sequence);
        Ok(drawn)
    }

    fn report_at(
        &mut self,
        millis: u64,
        input: SchedulerInput,
    ) -> Result<FrameReport, MissingOutput> {
        match self.at(millis, input).as_slice() {
            [SchedulerOutput::Report(report)] => Ok(**report),
            _ => Err(MissingOutput),
        }
    }

    fn next_wake_at(&self, millis: u64) -> Option<Duration> {
        self.0.next_wake(ms(millis))
    }
}

fn wants(view: ViewState, deadline: Option<Duration>) -> SchedulerInput {
    SchedulerInput::Wants {
        surface: WINDOW,
        view,
        deadline,
    }
}

fn published(next_change: Option<Duration>) -> SchedulerInput {
    SchedulerInput::Published {
        surface: WINDOW,
        next_change,
    }
}

fn built(sequence: u64) -> SchedulerInput {
    SchedulerInput::Built {
        surface: WINDOW,
        sequence: Some(FrameSequence(sequence)),
    }
}

fn rendered(frame: FrameTime, outcome: FrameOutcome, cost: Duration) -> SchedulerInput {
    SchedulerInput::Rendered {
        surface: WINDOW,
        frame,
        outcome,
        cost,
    }
}

fn shown(frame: FrameTime, cost: Duration) -> SchedulerInput {
    rendered(frame, FrameOutcome::Shown, cost)
}

fn display_ready() -> SchedulerInput {
    SchedulerInput::Timing(TimingEvent::DisplayReady(WINDOW))
}

fn presented(
    frame: FrameTime,
    time: Option<Duration>,
    refresh: Option<Duration>,
) -> SchedulerInput {
    SchedulerInput::Timing(TimingEvent::Presented {
        surface: WINDOW,
        frame,
        time,
        refresh,
        flags: PresentationFlags::default(),
    })
}

fn output_refresh(refresh: Option<Duration>) -> SchedulerInput {
    SchedulerInput::OutputRefresh {
        surface: WINDOW,
        refresh,
    }
}

/// A script whose window showed one frame 16 ms after it started, on a display without a fixed refresh.
fn presented_after_16_ms() -> Result<Script, MissingOutput> {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, shown(first, ms(1))).is_empty());
    let _presented_report = script.report_at(16, presented(first, Some(ms(16)), None))?;
    Ok(script)
}

#[test]
fn a_view_composes_again_only_once_the_display_is_ready() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.compose_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, wants(ViewState::Changed, None)).is_empty());
    let _drawn = script.render_at(2, built(first.sequence))?;
    assert!(script.at(3, wants(ViewState::Changed, None)).is_empty());
    assert!(script.at(4, shown(first, ms(4))).is_empty());
    assert!(script.at(5, wants(ViewState::Changed, None)).is_empty());
    assert!(script.at(16, display_ready()).is_empty());
    let second = script.compose_at(16, wants(ViewState::Changed, None))?;
    assert_eq!(second.sequence, first.sequence + 1);
    Ok(())
}

#[test]
fn a_view_that_needs_nothing_composes_nothing_when_the_display_is_ready() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(4, shown(first, ms(4))).is_empty());
    assert!(script.at(16, display_ready()).is_empty());
    assert!(script.at(16, wants(ViewState::Unchanged, None)).is_empty());
    Ok(())
}

#[test]
fn a_continuous_redraw_composes_an_unchanged_view() -> TestResult {
    let mut script = Script::opened(Redraw::Continuous);
    let _frame = script.compose_at(0, wants(ViewState::Unchanged, None))?;
    Ok(())
}

#[test]
fn an_unconfigured_surface_starts_no_frame() {
    let mut script = Script::opened(Redraw::Continuous);
    assert!(script.at(0, published(Some(ms(0)))).is_empty());
    assert!(
        script
            .at(0, wants(ViewState::Unconfigured, None))
            .is_empty()
    );
}

#[test]
fn with_a_refresh_a_frame_targets_the_first_refresh_after_its_render_cost() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(100, wants(ViewState::Changed, None))?;
    assert!(script.at(104, shown(first, ms(4))).is_empty());
    let report = script.report_at(111, presented(first, Some(ms(110)), Some(ms(10))))?;
    assert_eq!(report.stage, FrameStage::Presented);
    assert_eq!(report.observed, ms(110));
    assert!(script.at(117, display_ready()).is_empty());
    let second = script.compose_at(117, wants(ViewState::Changed, None))?;
    assert_eq!(second.presentation, ms(130));
    assert_eq!(second.refresh, Some(ms(10)));
    Ok(())
}

#[test]
fn the_render_cost_is_the_reported_cost() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(100, wants(ViewState::Changed, None))?;
    assert!(script.at(150, shown(first, ms(4))).is_empty());
    let _presented_report = script.report_at(160, presented(first, Some(ms(160)), Some(ms(10))))?;
    assert!(script.at(165, display_ready()).is_empty());
    let second = script.compose_at(165, wants(ViewState::Changed, None))?;
    assert_eq!(second.presentation, ms(170));
    Ok(())
}

#[test]
fn without_a_refresh_a_frame_targets_the_measured_presentation_delay() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(script.at(40, display_ready()).is_empty());
    let second = script.compose_at(40, wants(ViewState::Changed, None))?;
    assert_eq!(second.presentation, ms(56));
    assert_eq!(second.refresh, None);
    Ok(())
}

#[test]
fn a_presentation_without_a_time_is_reported_on_arrival_and_sets_the_refresh() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(2, shown(first, ms(2))).is_empty());
    let report = script.report_at(30, presented(first, None, Some(ms(8))))?;
    assert_eq!(report.observed, ms(30));
    assert_eq!(report.presentation_time, None);
    assert!(script.at(31, display_ready()).is_empty());
    let second = script.compose_at(31, wants(ViewState::Changed, None))?;
    assert_eq!(second.refresh, Some(ms(8)));
    Ok(())
}

#[test]
fn a_discarded_frame_is_reported_and_leaves_the_prediction_unchanged() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(script.at(20, display_ready()).is_empty());
    let second = script.drawn_at(20, wants(ViewState::Changed, None))?;
    assert!(script.at(22, shown(second, ms(2))).is_empty());
    let discarded = TimingEvent::Discarded {
        surface: WINDOW,
        frame: second,
    };
    let report = script.report_at(30, SchedulerInput::Timing(discarded))?;
    assert_eq!(report.stage, FrameStage::Discarded);
    assert_eq!(report.observed, ms(30));
    assert_eq!(report.timing.sequence, second.sequence);
    assert!(script.at(40, display_ready()).is_empty());
    let third = script.compose_at(40, wants(ViewState::Changed, None))?;
    assert_eq!(third.presentation, ms(56));
    Ok(())
}

#[test]
fn a_stale_frame_draws_again_at_once() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    let again = script.compose_at(1, rendered(first, FrameOutcome::Stale, ms(1)))?;
    assert_eq!(again.sequence, first.sequence + 1);
    Ok(())
}

#[test]
fn a_stale_frame_of_the_scene_composes_again() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    assert!(script.at(0, published(Some(ms(0)))).is_empty());
    let first = script.render_at(0, wants(ViewState::Unchanged, None))?;
    assert!(script.at(1, published(None)).is_empty());
    let again = script.compose_at(2, rendered(first, FrameOutcome::Stale, ms(2)))?;
    assert_eq!(again.sequence, first.sequence + 1);
    Ok(())
}

#[test]
fn a_pending_frame_draws_again_with_the_next_report() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(
        script
            .at(1, rendered(first, FrameOutcome::Pending, ms(1)))
            .is_empty()
    );
    let _again = script.compose_at(2, wants(ViewState::Changed, None))?;
    Ok(())
}

#[test]
fn a_report_about_another_frame_changes_nothing() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    let other = FrameTime {
        sequence: first.sequence + 1,
        ..first
    };
    assert!(
        script
            .at(1, rendered(other, FrameOutcome::Stale, ms(1)))
            .is_empty()
    );
    assert!(script.at(2, wants(ViewState::Changed, None)).is_empty());
    Ok(())
}

#[test]
fn a_composed_frame_renders_once_its_scene_is_built() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.compose_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, shown(first, ms(1))).is_empty());
    assert!(script.at(1, built(first.sequence - 1)).is_empty());
    let outside_compose = SchedulerInput::Built {
        surface: WINDOW,
        sequence: None,
    };
    assert!(script.at(1, outside_compose).is_empty());
    let drawn = script.render_at(2, built(first.sequence))?;
    assert_eq!(drawn.sequence, first.sequence);
    assert!(script.at(3, built(first.sequence)).is_empty());
    Ok(())
}

#[test]
fn a_scene_built_after_its_frame_rendered_renders_with_the_next_frame() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, built(first.sequence)).is_empty());
    assert!(script.at(2, shown(first, ms(2))).is_empty());
    assert!(script.at(16, display_ready()).is_empty());
    let again = script.render_at(16, wants(ViewState::Unchanged, None))?;
    assert!(script.at(17, shown(again, ms(1))).is_empty());
    assert!(script.at(32, display_ready()).is_empty());
    assert!(script.at(32, wants(ViewState::Unchanged, None)).is_empty());
    Ok(())
}

#[test]
fn a_moving_scene_renders_without_composing_the_view() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    assert!(script.at(0, published(Some(ms(50)))).is_empty());
    assert!(script.at(0, wants(ViewState::Unchanged, None)).is_empty());
    let frame = script.render_at(50, wants(ViewState::Unchanged, None))?;
    assert_eq!(frame.presentation, ms(50));
    Ok(())
}

#[test]
fn the_outputs_set_the_refresh_of_a_frame() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    assert!(script.at(0, output_refresh(Some(ms(16)))).is_empty());
    let first = script.compose_at(0, wants(ViewState::Changed, None))?;
    assert_eq!(first.refresh, Some(ms(16)));
    Ok(())
}

#[test]
fn a_deadline_of_the_view_wakes_at_the_deadline() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(
        script
            .at(100, wants(ViewState::Unchanged, Some(ms(150))))
            .is_empty()
    );
    assert_eq!(script.next_wake_at(100), Some(ms(150)));
    Ok(())
}

#[test]
fn a_report_of_the_view_while_drawing_replaces_the_deadline() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let _frame = script.compose_at(0, wants(ViewState::Changed, Some(ms(50))))?;
    assert!(
        script
            .at(1, wants(ViewState::Unchanged, Some(ms(30))))
            .is_empty()
    );
    assert_eq!(script.next_wake_at(1), Some(ms(30)));
    Ok(())
}

#[test]
fn a_change_of_the_scene_wakes_one_presentation_lead_early() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(script.at(100, published(Some(ms(150)))).is_empty());
    assert_eq!(script.next_wake_at(100), Some(ms(134)));
    Ok(())
}

#[test]
fn the_earlier_of_deadline_and_change_wakes() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(
        script
            .at(100, wants(ViewState::Unchanged, Some(ms(150))))
            .is_empty()
    );
    assert!(script.at(100, published(Some(ms(140)))).is_empty());
    assert_eq!(script.next_wake_at(100), Some(ms(124)));
    Ok(())
}

#[test]
fn a_time_that_has_passed_wakes_nothing() -> TestResult {
    let mut script = presented_after_16_ms()?;
    assert!(
        script
            .at(100, wants(ViewState::Unchanged, Some(ms(100))))
            .is_empty()
    );
    assert!(script.at(100, published(Some(ms(110)))).is_empty());
    assert_eq!(script.next_wake_at(100), None);
    Ok(())
}

#[test]
fn a_closed_surface_needs_no_frame_and_wakes_nothing() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let first = script.compose_at(0, wants(ViewState::Changed, Some(ms(50))))?;
    assert!(script.at(1, SchedulerInput::Closed(WINDOW)).is_empty());
    assert_eq!(script.next_wake_at(1), None);
    assert!(
        script
            .at(2, rendered(first, FrameOutcome::Stale, ms(2)))
            .is_empty()
    );
    assert!(script.at(3, display_ready()).is_empty());
    Ok(())
}

#[test]
fn a_closed_surface_ignores_later_reports() {
    let mut script = Script::opened(Redraw::Continuous);
    assert!(script.at(0, SchedulerInput::Closed(WINDOW)).is_empty());
    assert!(script.at(1, output_refresh(Some(ms(16)))).is_empty());
    assert!(script.at(1, published(Some(ms(40)))).is_empty());
    assert!(
        script
            .at(1, wants(ViewState::Changed, Some(ms(50))))
            .is_empty()
    );
    assert_eq!(script.next_wake_at(1), None);
}

mod commits;
