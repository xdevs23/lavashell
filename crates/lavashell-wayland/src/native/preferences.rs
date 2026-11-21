// SPDX-License-Identifier: GPL-3.0-only
use crate::translator::ViewLink;
use icu_locale_core::LanguageIdentifier;
use lavashell_platform::SettingsEvent;

fn user_languages() -> Vec<LanguageIdentifier> {
    languages(sys_locale::get_locales())
}

fn languages(tags: impl Iterator<Item = String>) -> Vec<LanguageIdentifier> {
    tags.filter_map(|tag| tag.parse().ok()).collect()
}

pub(crate) fn report_user_languages(view: &ViewLink) {
    view.report(SettingsEvent::Locales(user_languages()));
}

#[cfg(test)]
mod tests;
