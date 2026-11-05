// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use lavashell_core::prelude::Modifier;
use lavashell_core::units::DpExt;
use lavashell_test_fixtures::NoText;
use std::collections::BTreeSet;
use std::rc::Rc;

fn empty_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

fn open(records: &mut SurfaceRecords) -> Result<SurfaceId, TestError> {
    let size = Size::new(100.dp(), 50.dp());
    Ok(records.open(empty_view(), size, Density::ONE)?)
}

/// Frames the view of `surface`, so that it needs no frame until something invalidates it.
fn frame(records: &mut SurfaceRecords, surface: SurfaceId) -> TestResult {
    let viewport = records.open_viewport(surface).ok_or(TestError::NotOpen)?;
    let view = records.open_view_mut(surface).ok_or(TestError::NotOpen)?;
    let framed = view.frame(viewport, &mut NoText)?;
    assert_eq!(framed.failures.into_iter().count(), 0);
    assert!(!view.needs_visual_frame());
    Ok(())
}

fn needs_frame(records: &SurfaceRecords, surface: SurfaceId) -> bool {
    records
        .open_view(surface)
        .is_some_and(View::needs_visual_frame)
}

/// Closes `surface` and retires it once the destruction it asks for is done.
fn close_and_retire(records: &mut SurfaceRecords, surface: SurfaceId) {
    let destroy = records.handle(surface, SurfaceEvent::Closed);
    assert_eq!(destroy, Some(SurfaceRequest::Destroy(surface)));
    records.retire(surface);
}

fn configure(width: Option<Dp>) -> SurfaceEvent {
    SurfaceEvent::Configured {
        width,
        height: None,
    }
}

#[test]
fn ids_count_up_and_a_closed_id_is_not_reused() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let first = open(&mut records)?;
    close_and_retire(&mut records, first);
    let second = open(&mut records)?;
    assert_eq!((first, second), (SurfaceId(0), SurfaceId(1)));
    assert!(!records.is_open(first));
    assert!(records.is_open(second));
    Ok(())
}

#[test]
fn a_configure_sets_the_lengths_it_names_and_keeps_the_others() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    assert_eq!(records.handle(surface, configure(Some(200.dp()))), None);
    let expected = Viewport::new(Size::new(200.dp(), 50.dp()), Density::ONE);
    assert_eq!(records.open_viewport(surface), Some(expected));
    Ok(())
}

#[test]
fn a_configure_configures_a_waiting_surface_and_invalidates_its_view() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    frame(&mut records, surface)?;
    assert!(!records.is_configured(surface));
    assert_eq!(records.handle(surface, configure(None)), None);
    assert!(records.is_configured(surface));
    assert!(needs_frame(&records, surface));
    Ok(())
}

#[test]
fn a_density_change_sets_the_density_and_invalidates_the_view() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    frame(&mut records, surface)?;
    let density = Density::from_ratio(2.0, 1.0)?;
    let changed = SurfaceEvent::DensityChanged(density);
    assert_eq!(records.handle(surface, changed), None);
    let viewport = records.open_viewport(surface).ok_or(TestError::NotOpen)?;
    assert_eq!(viewport.density, density);
    assert!(needs_frame(&records, surface));
    Ok(())
}

#[test]
fn a_close_asks_once_for_the_destruction_and_later_events_change_nothing() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    let requests = [
        records.handle(surface, SurfaceEvent::Closed),
        records.handle(surface, SurfaceEvent::Closed),
        records.handle(surface, configure(Some(300.dp()))),
    ];
    let destroy = Some(SurfaceRequest::Destroy(surface));
    assert_eq!(requests, [destroy, None, None]);
    assert!(!records.is_configured(surface));
    assert!(!records.any_open());
    assert_eq!(records.open_viewport(surface), None);
    Ok(())
}

#[test]
fn the_view_shown_by_the_app_is_dropped_only_once_the_surface_retires() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let held = Rc::new(10.dp());
    let captured = held.clone();
    let view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.spacer(*captured, *captured, Modifier::new());
    });
    let surface = records.open(view, Size::new(10.dp(), 10.dp()), Density::ONE)?;
    assert_eq!(Rc::strong_count(&held), 2);
    assert!(records.handle(surface, SurfaceEvent::Closed).is_some());
    assert_eq!(Rc::strong_count(&held), 2);
    assert!(records.open_view(surface).is_none());
    records.retire(surface);
    assert_eq!(Rc::strong_count(&held), 1);
    assert!(records.view_mut(surface).is_none());
    Ok(())
}

#[test]
fn surfaces_opened_and_retired_many_times_free_their_records_and_get_new_ids() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let window = open(&mut records)?;
    let mut ids = BTreeSet::from([window]);
    for _ in 0..100 {
        let popup = open(&mut records)?;
        assert!(ids.insert(popup));
        assert_eq!(records.handle(popup, configure(None)), None);
        close_and_retire(&mut records, popup);
        assert_eq!(records.records.len(), 1);
    }
    assert!(records.is_open(window));
    Ok(())
}

#[test]
fn a_late_event_for_a_retired_surface_changes_nothing() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let retired = open(&mut records)?;
    let other = open(&mut records)?;
    let viewport = records.open_viewport(other);
    close_and_retire(&mut records, retired);
    let density = Density::from_ratio(2.0, 1.0)?;
    let late = [
        records.handle(retired, SurfaceEvent::Closed),
        records.handle(retired, configure(Some(300.dp()))),
        records.handle(retired, SurfaceEvent::DensityChanged(density)),
    ];
    records.retire(retired);
    assert_eq!(late, [None, None, None]);
    assert!(!records.is_open(retired));
    assert!(!records.is_configured(retired));
    assert!(records.view_mut(retired).is_none());
    assert_eq!(records.open_viewport(other), viewport);
    assert!(records.is_open(other));
    assert_eq!(records.records.len(), 1);
    Ok(())
}

#[test]
fn opening_fails_once_every_surface_id_is_used() {
    let mut records = SurfaceRecords {
        last_id: Some(u64::MAX - 1),
        ..SurfaceRecords::new(PreferenceOverrides::default())
    };
    let last = records.open(empty_view(), Size::ZERO, Density::ONE);
    assert!(matches!(last, Ok(SurfaceId(u64::MAX))));
    let opened = records.open(empty_view(), Size::ZERO, Density::ONE);
    assert!(matches!(opened, Err(Error::NumberSurface)));
    assert_eq!(records.records.len(), 1);
}

#[test]
fn only_a_closing_surface_retires() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    records.retire(surface);
    assert!(records.is_open(surface));
    Ok(())
}

#[test]
fn an_unknown_surface_has_no_record() {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    assert_eq!(records.handle(SurfaceId(3), SurfaceEvent::Closed), None);
    assert!(records.view_mut(SurfaceId(3)).is_none());
    assert!(!records.any_open());
}
