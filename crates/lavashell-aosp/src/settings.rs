// SPDX-License-Identifier: GPL-3.0-only
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use icu_locale_core::{LanguageIdentifier, Locale};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_platform::{Reports, SettingsEvent};

#[derive(Clone, Debug, PartialEq)]
pub struct HostConfiguration {
    pub color_scheme: ColorScheme,
    pub locales: Vec<LanguageIdentifier>,
}

pub(crate) struct ConfigurationReading {
    pub(crate) night_mode_active: bool,
    pub(crate) language_tags: Vec<String>,
}

impl From<ConfigurationReading> for HostConfiguration {
    fn from(reading: ConfigurationReading) -> Self {
        let color_scheme = if reading.night_mode_active {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        let locales = (reading.language_tags.into_iter())
            .filter_map(|tag| tag.parse::<Locale>().ok())
            .map(|locale| locale.id)
            .collect();
        Self {
            color_scheme,
            locales,
        }
    }
}

pub(crate) fn report_configuration(
    configuration: HostConfiguration,
    reports: &Reports<NativeWindow, AospStreams>,
) {
    let appearance = Appearance {
        color_scheme: Some(configuration.color_scheme),
        ..Appearance::default()
    };
    reports.report(SettingsEvent::Appearance(appearance));
    reports.report(SettingsEvent::Locales(configuration.locales));
}
