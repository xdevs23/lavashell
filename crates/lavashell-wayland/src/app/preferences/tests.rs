// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::preference_reader::PreferenceReader;
use icu_locale_core::langid;
use lavashell_core::preferences::{
    ContrastPreference, PreferenceOverrides, PreferenceValues, TextScaling,
};
use lavashell_core::units::{DpExt, SpExt};

fn configuration(system: PreferenceValues, overrides: PreferenceOverrides) -> ViewConfiguration {
    let mut preferences = PreferenceSource::default();
    preferences.set_system(system);
    preferences.set_overrides(overrides);
    ViewConfiguration {
        preferences,
        facts: HostFacts::default(),
    }
}

#[test]
fn newly_created_views_inherit_explicit_overrides_and_current_system_preferences() {
    let configuration = configuration(
        PreferenceValues {
            reduced_motion: true,
            ..Default::default()
        },
        PreferenceOverrides {
            text_scaling: TextScaling::linear(1.5),
            reduced_motion: Some(false),
            reduced_transparency: Some(true),
            contrast: Some(ContrastPreference::More),
        },
    );
    let mut reader = PreferenceReader::new(lavashell_test_platform::facts());
    assert_eq!(
        configuration.apply(&reader.view, Instant::now()),
        lavashell_test_platform::facts()
    );
    let actual = reader.values();
    assert_eq!(actual.text_scaling.to_dp(12.sp()), 18.dp());
    assert!(!actual.reduced_motion);
    assert!(actual.reduced_transparency);
    assert_eq!(actual.contrast, ContrastPreference::More);
}

#[test]
fn a_system_update_reaches_every_view_and_leaves_the_overrides_in_place() {
    let mut configuration = configuration(
        PreferenceValues::default(),
        PreferenceOverrides {
            reduced_motion: Some(false),
            ..Default::default()
        },
    );
    let mut readers = [
        PreferenceReader::new(lavashell_test_platform::facts()),
        PreferenceReader::new(lavashell_test_platform::facts()),
    ];
    let supplied: Vec<_> = (readers.iter())
        .map(|reader| configuration.apply(&reader.view, Instant::now()))
        .collect();
    let appearance = Appearance {
        reduced_motion: true,
        contrast: ContrastPreference::More,
        color_scheme: None,
    };
    let views = readers.iter().map(|reader| &reader.view).zip(&supplied);
    configuration.update(appearance, views);
    for reader in &mut readers {
        let actual = reader.values();
        assert!(!actual.reduced_motion);
        assert_eq!(actual.contrast, ContrastPreference::More);
    }
}

#[test]
fn host_facts_replace_the_supplied_ones_while_the_host_knows_them() {
    let mut configuration =
        configuration(PreferenceValues::default(), PreferenceOverrides::default());
    let reader = PreferenceReader::new(lavashell_test_platform::facts());
    let supplied = configuration.apply(&reader.view, Instant::now());
    let scheme = |color_scheme| Appearance {
        color_scheme,
        ..Appearance::default()
    };
    let other = match supplied.color_scheme {
        ColorScheme::Light => ColorScheme::Dark,
        ColorScheme::Dark => ColorScheme::Light,
    };
    configuration.update(scheme(Some(other)), [(&reader.view, &supplied)].into_iter());
    assert_eq!(reader.view.platform().color_scheme, other);
    configuration.repeat(
        HostRepeat::Disabled,
        [(&reader.view, &supplied)].into_iter(),
    );
    assert_eq!(reader.view.platform().key_repeat, None);
    configuration.update(scheme(None), [(&reader.view, &supplied)].into_iter());
    assert_eq!(reader.view.platform().color_scheme, supplied.color_scheme);
    let new = PreferenceReader::new(lavashell_test_platform::facts());
    assert_eq!(
        configuration.apply(&new.view, Instant::now()),
        lavashell_test_platform::facts()
    );
    assert_eq!(new.view.platform().key_repeat, None);
}

#[test]
fn the_languages_of_the_environment_replace_the_supplied_ones() {
    let mut configuration =
        configuration(PreferenceValues::default(), PreferenceOverrides::default());
    let reader = PreferenceReader::new(lavashell_test_platform::facts());
    configuration.facts.locales = Some(vec![langid!("pt-BR"), langid!("de")]);
    assert_eq!(
        configuration.apply(&reader.view, Instant::now()).locales,
        lavashell_test_platform::facts().locales
    );
    assert_eq!(
        reader.view.platform().locales,
        [langid!("pt-BR"), langid!("de")]
    );
}

#[test]
fn locale_tags_that_name_no_language_are_left_out() {
    let tags = ["de-DE", "C", "en"].map(str::to_owned);
    assert_eq!(
        languages(tags.into_iter()),
        Some(vec![langid!("de-DE"), langid!("en")])
    );
    assert_eq!(languages(["C".to_owned()].into_iter()), None);
}
