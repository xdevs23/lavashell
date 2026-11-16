// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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
fn with_a_refresh_the_wake_before_a_change_of_the_scene_stays_while_time_passes() -> TestResult {
    let mut script = presented_on_a_10_ms_refresh()?;
    assert!(script.at(100, published(Some(ms(150)))).is_empty());
    for millis in [100, 103, 107] {
        assert_eq!(script.next_wake_at(millis), Some(ms(145)));
    }
    assert!(script.at(103, wants(ViewState::Unchanged, None)).is_empty());
    assert!(script.at(107, wants(ViewState::Unchanged, None)).is_empty());
    assert_eq!(script.wakes, [Some(ms(145))]);
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
