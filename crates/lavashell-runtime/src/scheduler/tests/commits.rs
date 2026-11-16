// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn state_changed() -> SchedulerInput {
    SchedulerInput::StateChanged(WINDOW)
}

fn committed() -> SchedulerInput {
    SchedulerInput::Committed(WINDOW)
}

fn starts_commit(outputs: &[SchedulerOutput]) -> bool {
    matches!(outputs, [SchedulerOutput::CommitState(WINDOW)])
}

#[test]
fn a_state_change_outside_a_frame_starts_one_commit_at_a_time() {
    let mut script = Script::opened(Redraw::OnChange);
    assert!(starts_commit(&script.at(0, state_changed())));
    assert!(script.at(1, state_changed()).is_empty());
    assert!(starts_commit(&script.at(2, committed())));
    assert!(script.at(3, committed()).is_empty());
}

#[test]
fn a_state_change_while_a_frame_composes_applies_with_the_draw_of_that_frame() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let composed = script.compose_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, state_changed()).is_empty());
    let drawn = script.render_at(2, built(composed.sequence))?;
    assert!(script.at(3, shown(drawn, ms(1))).is_empty());
    Ok(())
}

#[test]
fn a_state_change_while_a_frame_is_drawn_commits_once_the_frame_is_reported() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let drawn = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, state_changed()).is_empty());
    assert!(starts_commit(&script.at(2, shown(drawn, ms(1)))));
    Ok(())
}

#[test]
fn a_state_change_waits_for_the_compose_that_a_stale_frame_starts() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    let drawn = script.drawn_at(0, wants(ViewState::Changed, None))?;
    assert!(script.at(1, state_changed()).is_empty());
    let again = script.compose_at(2, rendered(drawn, FrameOutcome::Stale, ms(1)))?;
    let redrawn = script.render_at(3, built(again.sequence))?;
    assert!(script.at(4, shown(redrawn, ms(1))).is_empty());
    Ok(())
}
