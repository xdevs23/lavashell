// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::Size;
use lavashell_core::output::{Millimetres, PhysicalSize};
use lavashell_core::units::DpExt;

const MONITOR: OutputId = OutputId(7);
const OTHER_MONITOR: OutputId = OutputId(9);
const WINDOW: SurfaceId = SurfaceId(1);
const OTHER_WINDOW: SurfaceId = SurfaceId(2);
const FAST: Duration = Duration::from_millis(7);
const SLOW: Duration = Duration::from_millis(17);

fn info(name: &str, width: i32) -> MonitorInfo {
    MonitorInfo {
        name: Some(name.to_owned()),
        size: Size::new(width.dp(), 1080.dp()),
        physical_size: Some(PhysicalSize {
            width: Millimetres(600.0),
            height: Millimetres(340.0),
        }),
    }
}

fn refreshes(records: &OutputRecords, surface: SurfaceId) -> Vec<Duration> {
    records.refreshes(surface).collect()
}

fn add_monitor(records: &mut OutputRecords, output: OutputId, refresh: Option<Duration>) {
    records.handle(OutputEvent::Updated(output, info("DP-1", 1920)));
    records.handle(OutputEvent::Refresh(output, refresh));
}

#[test]
fn an_updated_output_has_its_geometry() {
    let mut records = OutputRecords::default();
    assert_eq!(records.info(MONITOR), None);
    records.handle(OutputEvent::Updated(MONITOR, info("DP-1", 1920)));
    assert_eq!(records.info(MONITOR), Some(&info("DP-1", 1920)));
}

#[test]
fn an_update_replaces_the_geometry_and_keeps_the_refresh_and_the_surfaces() {
    let mut records = OutputRecords::default();
    add_monitor(&mut records, MONITOR, Some(FAST));
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    records.handle(OutputEvent::Updated(MONITOR, info("DP-1", 2560)));
    assert_eq!(records.info(MONITOR), Some(&info("DP-1", 2560)));
    assert_eq!(refreshes(&records, WINDOW), [FAST]);
}

#[test]
fn an_output_without_geometry_records_its_surfaces_and_refresh() {
    let mut records = OutputRecords::default();
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    records.handle(OutputEvent::Refresh(MONITOR, Some(FAST)));
    assert_eq!(records.info(MONITOR), None);
    assert_eq!(refreshes(&records, WINDOW), [FAST]);
}

#[test]
fn geometry_that_arrives_after_a_surface_entered_keeps_the_surface_and_the_refresh() {
    let mut records = OutputRecords::default();
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    records.handle(OutputEvent::Refresh(MONITOR, Some(FAST)));
    records.handle(OutputEvent::Updated(MONITOR, info("DP-1", 1920)));
    assert_eq!(records.info(MONITOR), Some(&info("DP-1", 1920)));
    assert_eq!(refreshes(&records, WINDOW), [FAST]);
}

#[test]
fn a_removed_output_is_unknown_and_loses_its_surfaces() {
    let mut records = OutputRecords::default();
    add_monitor(&mut records, MONITOR, Some(FAST));
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    records.handle(OutputEvent::Removed(MONITOR));
    assert_eq!(records.info(MONITOR), None);
    add_monitor(&mut records, MONITOR, Some(FAST));
    assert_eq!(refreshes(&records, WINDOW), []);
}

#[test]
fn a_surface_has_the_refresh_of_each_output_it_is_shown_on() {
    let mut records = OutputRecords::default();
    add_monitor(&mut records, MONITOR, Some(SLOW));
    add_monitor(&mut records, OTHER_MONITOR, Some(FAST));
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    assert_eq!(refreshes(&records, WINDOW), [SLOW]);
    records.handle(OutputEvent::Entered(OTHER_MONITOR, WINDOW));
    assert_eq!(refreshes(&records, WINDOW), [SLOW, FAST]);
    assert_eq!(refreshes(&records, OTHER_WINDOW), []);
    records.handle(OutputEvent::Left(MONITOR, WINDOW));
    assert_eq!(refreshes(&records, WINDOW), [FAST]);
}

#[test]
fn an_output_without_a_refresh_adds_nothing() {
    let mut records = OutputRecords::default();
    add_monitor(&mut records, MONITOR, None);
    records.handle(OutputEvent::Entered(MONITOR, WINDOW));
    assert_eq!(refreshes(&records, WINDOW), []);
    records.handle(OutputEvent::Refresh(MONITOR, Some(FAST)));
    assert_eq!(refreshes(&records, WINDOW), [FAST]);
    records.handle(OutputEvent::Refresh(MONITOR, None));
    assert_eq!(refreshes(&records, WINDOW), []);
}

#[test]
fn a_forgotten_surface_is_on_no_output() {
    let mut records = OutputRecords::default();
    add_monitor(&mut records, MONITOR, Some(FAST));
    add_monitor(&mut records, OTHER_MONITOR, Some(SLOW));
    for output in [MONITOR, OTHER_MONITOR] {
        records.handle(OutputEvent::Entered(output, WINDOW));
        records.handle(OutputEvent::Entered(output, OTHER_WINDOW));
    }
    records.forget(WINDOW);
    assert_eq!(refreshes(&records, WINDOW), []);
    assert_eq!(refreshes(&records, OTHER_WINDOW), [FAST, SLOW]);
}
