// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn a_continuous_redraw_composes_again_one_refresh_after_an_unrendered_frame_started() -> TestResult
{
    let mut script = Script::opened(Redraw::Continuous);
    assert!(script.at(0, output_refresh(Some(ms(10)))).is_empty());
    let first = script.drawn_at(2, wants(ViewState::Unchanged, None))?;
    assert!(starts_commit(&script.at(3, unrendered(first))));
    assert!(script.at(4, wants(ViewState::Unchanged, None)).is_empty());
    assert_eq!(script.next_wake_at(4), Some(ms(12)));
    let second = script.compose_at(12, wants(ViewState::Unchanged, None))?;
    assert_eq!(second.sequence, first.sequence + 1);
    Ok(())
}

#[test]
fn without_a_refresh_only_a_change_starts_a_frame_after_an_unrendered_frame() -> TestResult {
    let mut script = Script::opened(Redraw::Continuous);
    let first = script.drawn_at(0, wants(ViewState::Unchanged, None))?;
    assert!(starts_commit(&script.at(1, unrendered(first))));
    assert!(script.at(2, wants(ViewState::Unchanged, None)).is_empty());
    assert_eq!(script.next_wake_at(2), None);
    let _changed_frame_is_not_checked = script.compose_at(3, wants(ViewState::Changed, None))?;
    Ok(())
}

#[test]
fn the_state_of_an_unrendered_frame_starts_committing_before_the_next_frame() -> TestResult {
    let mut script = Script::opened(Redraw::OnChange);
    assert!(starts_commit(
        &script.at(0, SchedulerInput::StateChanged(WINDOW))
    ));
    let first = script.drawn_at(1, wants(ViewState::Changed, None))?;
    assert!(script.at(2, unrendered(first)).is_empty());
    assert!(script.at(3, wants(ViewState::Changed, None)).is_empty());
    assert!(starts_commit(
        &script.at(4, SchedulerInput::Committed(WINDOW))
    ));
    let second = script.compose_at(5, wants(ViewState::Changed, None))?;
    assert_eq!(second.sequence, first.sequence + 1);
    Ok(())
}
