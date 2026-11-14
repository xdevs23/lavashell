// SPDX-License-Identifier: GPL-3.0-only
//! The configuration reported by an AOSP host, and the settings events that report it to the platform.

use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use icu_locale_core::{LanguageIdentifier, Locale};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_platform::{Reports, SettingsEvent};

/// The configuration read from the resources of the host app. `configuration_of` reads it from an
/// android.content.Context.
#[derive(Clone, Debug, PartialEq)]
pub struct HostConfiguration {
    /// `Dark` while the night mode of the configuration is active.
    pub color_scheme: ColorScheme,
    /// The locales of the configuration, most preferred first.
    pub locales: Vec<LanguageIdentifier>,
}

/// The values that `configuration_of` reads through JNI.
pub(crate) struct ConfigurationReading {
    pub(crate) night_mode_active: bool,
    /// The BCP 47 language tags of the locales, most preferred first.
    pub(crate) language_tags: Vec<String>,
}

/// The configuration described by `reading`. A language tag that is not a valid BCP 47 locale is skipped.
pub(crate) fn configuration_from(reading: ConfigurationReading) -> HostConfiguration {
    let color_scheme = if reading.night_mode_active {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    };
    let locales = (reading.language_tags.into_iter())
        .filter_map(|tag| tag.parse::<Locale>().ok())
        .map(|locale| locale.id)
        .collect();
    HostConfiguration {
        color_scheme,
        locales,
    }
}

/// Reports `configuration` as the appearance and the locales of the host settings. The configuration contains only
/// the night mode as an appearance preference. `Appearance` cannot leave a preference out, so the others are sent with
/// their default values, which the host never read.
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
