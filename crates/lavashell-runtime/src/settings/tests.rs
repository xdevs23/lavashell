// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::SurfaceRecords;
use crate::preference_reader::PreferenceReader;
use crate::test_error::{TestError, TestResult};
use icu_locale_core::langid;
use lavashell_core::geometry::Size;
use lavashell_core::preferences::{Appearance, ContrastPreference, PreferenceValues, TextScaling};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, DpExt, SpExt};
use lavashell_platform::SurfaceEvent;

/// Opens a surface whose view records the preferences read by that view.
fn open_reader(records: &mut SurfaceRecords) -> Result<(SurfaceId, PreferenceReader), TestError> {
    let (view, reader) = PreferenceReader::view(lavashell_test_fixtures::facts());
    let size = Size::new(100.dp(), 50.dp());
    Ok((records.open(view, size, Density::ONE, None)?, reader))
}

fn read(
    records: &mut SurfaceRecords,
    (surface, reader): &(SurfaceId, PreferenceReader),
) -> Result<PreferenceValues, TestError> {
    let view = records.open_view_mut(*surface).ok_or(TestError::NotOpen)?;
    Ok(reader.values(view)?)
}

fn platform(records: &SurfaceRecords, surface: SurfaceId) -> Result<PlatformFacts, TestError> {
    let view = records.open_view(surface).ok_or(TestError::NotOpen)?;
    Ok(view.platform())
}

fn color_scheme(color_scheme: Option<ColorScheme>) -> SettingsEvent {
    SettingsEvent::Appearance(Appearance {
        color_scheme,
        ..Appearance::default()
    })
}

#[test]
fn a_new_view_gets_the_overrides_in_place_of_the_preferences_of_the_host() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides {
        text_scaling: TextScaling::linear(1.5),
        reduced_motion: Some(false),
        reduced_transparency: Some(true),
        contrast: Some(ContrastPreference::More),
    });
    records.change_settings(SettingsEvent::Appearance(Appearance {
        reduced_motion: true,
        ..Appearance::default()
    }));
    let reader = open_reader(&mut records)?;
    assert_eq!(
        platform(&records, reader.0)?,
        lavashell_test_fixtures::facts()
    );
    let actual = read(&mut records, &reader)?;
    assert_eq!(actual.text_scaling.to_dp(12.sp()), 18.dp());
    assert!(!actual.reduced_motion);
    assert!(actual.reduced_transparency);
    assert_eq!(actual.contrast, ContrastPreference::More);
    Ok(())
}

#[test]
fn an_appearance_reaches_every_open_view_and_leaves_the_overrides_in_place() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides {
        reduced_motion: Some(false),
        ..PreferenceOverrides::default()
    });
    let readers = [open_reader(&mut records)?, open_reader(&mut records)?];
    records.change_settings(SettingsEvent::Appearance(Appearance {
        reduced_motion: true,
        contrast: ContrastPreference::More,
        color_scheme: None,
    }));
    for reader in &readers {
        let actual = read(&mut records, reader)?;
        assert!(!actual.reduced_motion);
        assert_eq!(actual.contrast, ContrastPreference::More);
    }
    Ok(())
}

#[test]
fn host_facts_replace_the_supplied_ones_while_the_host_has_them() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let (surface, _reader) = open_reader(&mut records)?;
    let supplied = lavashell_test_fixtures::facts();
    let other = match supplied.color_scheme {
        ColorScheme::Light => ColorScheme::Dark,
        ColorScheme::Dark => ColorScheme::Light,
    };
    records.change_settings(color_scheme(Some(other)));
    assert_eq!(platform(&records, surface)?.color_scheme, other);
    records.change_settings(SettingsEvent::KeyRepeat(None));
    assert_eq!(platform(&records, surface)?.key_repeat, None);
    records.change_settings(color_scheme(None));
    assert_eq!(
        platform(&records, surface)?.color_scheme,
        supplied.color_scheme
    );
    let (new, _reader) = open_reader(&mut records)?;
    assert_eq!(platform(&records, new)?.key_repeat, None);
    Ok(())
}

#[test]
fn languages_reported_by_the_host_replace_the_supplied_ones_unless_empty() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let (surface, _reader) = open_reader(&mut records)?;
    let reported = vec![langid!("pt-BR"), langid!("de")];
    records.change_settings(SettingsEvent::Locales(reported.clone()));
    assert_eq!(platform(&records, surface)?.locales, reported);
    records.change_settings(SettingsEvent::Locales(Vec::new()));
    assert_eq!(
        platform(&records, surface)?.locales,
        lavashell_test_fixtures::facts().locales
    );
    Ok(())
}

#[test]
fn a_closed_surface_gets_no_settings() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let (surface, _reader) = open_reader(&mut records)?;
    assert!(records.handle(surface, SurfaceEvent::Closed).is_some());
    records.change_settings(SettingsEvent::KeyRepeat(None));
    let view = records.view_mut(surface).ok_or(TestError::NotOpen)?;
    assert_eq!(view.platform(), lavashell_test_fixtures::facts());
    Ok(())
}
