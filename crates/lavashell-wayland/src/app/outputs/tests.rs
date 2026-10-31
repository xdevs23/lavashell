// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn mode(dimensions: (i32, i32), refresh_rate: i32, current: bool) -> Mode {
    Mode {
        dimensions,
        refresh_rate,
        current,
        preferred: false,
    }
}

#[test]
fn a_reported_logical_size_wins_over_the_current_mode() {
    let modes = [mode((3840, 2160), 60_000, true)];
    let size = logical_size(Some((1280, 720)), &modes, 2);
    assert_eq!(size, Some((1280, 720)));
}

#[test]
fn the_current_mode_divided_by_the_scale_factor_is_the_logical_size() {
    let modes = [
        mode((1920, 1080), 144_000, false),
        mode((3840, 2160), 60_000, true),
    ];
    assert_eq!(logical_size(None, &modes, 2), Some((1920, 1080)));
}

#[test]
fn a_scale_factor_of_zero_counts_as_one() {
    let modes = [mode((3840, 2160), 60_000, true)];
    assert_eq!(logical_size(None, &modes, 0), Some((3840, 2160)));
}

#[test]
fn an_output_without_a_logical_size_or_a_current_mode_has_no_logical_size() {
    let modes = [mode((3840, 2160), 60_000, false)];
    assert_eq!(logical_size(None, &modes, 1), None);
    assert_eq!(logical_size(None, &[], 1), None);
}

#[test]
fn the_refresh_interval_comes_from_the_current_mode_in_millihertz() {
    let modes = [
        mode((1920, 1080), 144_000, false),
        mode((3840, 2160), 60_000, true),
    ];
    let interval = Duration::from_secs_f64(1.0 / 60.0);
    assert_eq!(refresh_interval(&modes), Some(interval));
}

#[test]
fn a_current_mode_with_a_refresh_rate_of_zero_has_no_refresh_interval() {
    assert_eq!(refresh_interval(&[mode((3840, 2160), 0, true)]), None);
}

#[test]
fn a_physical_size_has_its_width_and_height_in_millimetres() {
    let expected = PhysicalSize {
        width: Millimetres(600.0),
        height: Millimetres(340.0),
    };
    assert_eq!(physical_size((600, 340)), Some(expected));
}

#[test]
fn a_physical_size_with_a_dimension_of_zero_is_not_reported() {
    assert_eq!(physical_size((0, 0)), None);
    assert_eq!(physical_size((600, 0)), None);
}
