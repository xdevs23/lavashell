// SPDX-License-Identifier: GPL-3.0-only
use crate::settings::{ConfigurationReading, HostConfiguration, configuration_from};
use icu_locale_core::langid;
use lavashell_core::platform::ColorScheme;

#[test]
fn a_configuration_reading_keeps_the_night_mode_and_the_valid_locales() {
    let reading = ConfigurationReading {
        night_mode_active: true,
        language_tags: vec![
            "en-US-u-fw-mon".to_owned(),
            "C".to_owned(),
            "de-AT".to_owned(),
        ],
    };
    let configuration = HostConfiguration {
        color_scheme: ColorScheme::Dark,
        locales: vec![langid!("en-US"), langid!("de-AT")],
    };
    assert_eq!(configuration_from(reading), configuration);
}

#[test]
fn a_configuration_reading_without_the_night_mode_is_light() {
    let reading = ConfigurationReading {
        night_mode_active: false,
        language_tags: Vec::new(),
    };
    let configuration = HostConfiguration {
        color_scheme: ColorScheme::Light,
        locales: Vec::new(),
    };
    assert_eq!(configuration_from(reading), configuration);
}
