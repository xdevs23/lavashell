// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;

use super::test_error::SurfaceRecordsTestError;
use super::*;
use crate::test_error::TestResult;
use crate::test_host::empty_view;
use crate::view_actor::ViewError;
use lavashell_core::prelude::{Modifier, PixelSize};
use lavashell_core::units::DpExt;
use lavashell_core::window::{WINDOW, WindowHandle};
use lavashell_test_fixtures::{NoText, RecordedScene, WithoutFailures};
use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

fn open(records: &mut SurfaceRecords) -> Result<SurfaceId, ViewError> {
    let size = Size::new(100.dp(), 50.dp());
    Ok(records.open(empty_view(), size, Density::ONE, None, None)?)
}

fn frame(records: &mut SurfaceRecords, surface: SurfaceId) -> TestResult {
    let viewport = (records.open_viewport(surface)).ok_or(SurfaceRecordsTestError::NotOpen)?;
    let view = (records.open_view_mut(surface)).ok_or(SurfaceRecordsTestError::NotOpen)?;
    (view.frame_into(viewport, &mut NoText, &mut RecordedScene::default())).without_failures()?;
    assert!(!view.needs_visual_frame());
    Ok(())
}

fn needs_frame(records: &SurfaceRecords, surface: SurfaceId) -> bool {
    records
        .open_view(surface)
        .is_some_and(View::needs_visual_frame)
}

fn close_and_retire(records: &mut SurfaceRecords, surface: SurfaceId) {
    let destroy = records.handle(surface, SurfaceEvent::Closed);
    assert_eq!(destroy, Some(SurfaceRequest::Destroy(surface)));
    records.retire(surface);
}

fn configure(width: Option<Dp>) -> SurfaceEvent {
    SurfaceEvent::Configured {
        width,
        height: None,
        token: None,
        window: None,
    }
}

#[test]
fn ids_count_up_and_a_closed_id_is_not_reused() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let first = open(&mut records)?;
    close_and_retire(&mut records, first);
    let second = open(&mut records)?;
    assert_eq!(
        (first, second),
        (SurfaceId::numbered(0), SurfaceId::numbered(1))
    );
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
    let viewport = (records.open_viewport(surface)).ok_or(SurfaceRecordsTestError::NotOpen)?;
    assert_eq!(viewport.density, density);
    assert!(needs_frame(&records, surface));
    Ok(())
}

#[test]
fn only_a_window_configure_hands_a_window_to_the_view() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let read = Rc::new(Cell::new(None));
    let reader = read.clone();
    let view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        reader.set(Some(ui.local(&WINDOW).map(WindowHandle::state)));
    });
    let surface = records.open(view, Size::new(10.dp(), 10.dp()), Density::ONE, None, None)?;
    assert_eq!(records.handle(surface, configure(None)), None);
    frame(&mut records, surface)?;
    assert_eq!(read.get(), Some(None));
    let maximized = WindowState {
        maximized: true,
        ..WindowState::default()
    };
    let configure = SurfaceEvent::Configured {
        width: None,
        height: None,
        token: None,
        window: Some(maximized),
    };
    assert_eq!(records.handle(surface, configure), None);
    frame(&mut records, surface)?;
    assert_eq!(read.get(), Some(Some(maximized)));
    Ok(())
}

#[test]
fn a_new_surface_has_square_pixels_until_a_scale_change_sets_its_scale() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    frame(&mut records, surface)?;
    let square = (records.open_scaled_viewport(surface)).ok_or(SurfaceRecordsTestError::NotOpen)?;
    assert_eq!(square.scale, PixelScale::SQUARE);
    let wide = PixelScale::filling(square.viewport, PixelSize::new(160, 50))?;
    let changed = SurfaceEvent::ScaleChanged(wide);
    assert_eq!(records.handle(surface, changed), None);
    let scale = (records.open_scaled_viewport(surface)).map(|scaled| scaled.scale);
    assert_eq!(scale, Some(wide));
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
fn the_view_shown_by_the_app_is_dropped_only_once_the_surface_retires() -> Result<(), Error> {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let held = Rc::new(10.dp());
    let captured = held.clone();
    let view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.spacer(*captured, *captured, Modifier::new());
    });
    let surface = records.open(view, Size::new(10.dp(), 10.dp()), Density::ONE, None, None)?;
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
    assert_eq!(
        records.handle(SurfaceId::numbered(3), SurfaceEvent::Closed),
        None
    );
    assert!(records.view_mut(SurfaceId::numbered(3)).is_none());
    assert!(!records.any_open());
}
