// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, icon, lucide};
use clap::ValueEnum;
use lavashell::core::platform::COLOR_SCHEME;
use lavashell::prelude::*;

mod words {
    include!(concat!(env!("OUT_DIR"), "/copy.rs"));
}
use words::{CopyKey, copy};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    #[must_use]
    pub fn scheme(self, system: ColorScheme) -> ColorScheme {
        match self {
            Self::System => system,
            Self::Light => ColorScheme::Light,
            Self::Dark => ColorScheme::Dark,
        }
    }

    fn name(self) -> CopyKey {
        match self {
            Self::System => CopyKey::FollowSystem,
            Self::Light => CopyKey::Light,
            Self::Dark => CopyKey::Dark,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeSwitchColors {
    pub ink: Color,
    pub accent: Color,
}

#[derive(Clone)]
pub struct Theme {
    choice: State<ThemeChoice>,
    system: &'static Icon,
    light: &'static Icon,
    dark: &'static Icon,
}

impl Theme {
    pub fn new(choice: ThemeChoice) -> Result<Self, Error> {
        Ok(Self {
            choice: State::new(choice),
            system: icon(&lucide::MONITOR)?,
            light: icon(&lucide::SUN)?,
            dark: icon(&lucide::MOON)?,
        })
    }

    #[must_use]
    pub fn choice(&self) -> ThemeChoice {
        self.choice.get()
    }

    pub fn choose(&self, choice: ThemeChoice) {
        self.choice.set(choice);
    }

    #[must_use]
    pub fn scheme(&self, ui: &Ui) -> ColorScheme {
        self.choice().scheme(ui.local(&COLOR_SCHEME))
    }

    pub fn provide<T: Clone + PartialEq + 'static>(
        &self,
        ui: &mut Ui,
        local: &'static Local<T>,
        of: fn(ColorScheme) -> T,
        children: impl FnOnce(&mut Ui),
    ) {
        ui.provide(local, of(self.scheme(ui)), children);
    }

    pub fn switch(&self, ui: &mut Ui, colors: ThemeSwitchColors) {
        let current = self.choice();
        ui.row(
            Modifier::new().gap(2.dp()).align_items(Align::Center),
            |ui| {
                for choice in ThemeChoice::value_variants().iter().copied() {
                    let selected = choice == current;
                    ui.icon_button(
                        self.icon(choice),
                        IconOptions {
                            size: Some(16.dp()),
                            ..IconOptions::default()
                        },
                        Modifier::new()
                            .tag(choice)
                            .accessible_name(copy(choice.name()))
                            .selected(selected)
                            .color(if selected { colors.accent } else { colors.ink })
                            .clip(8.dp())
                            .background(if selected {
                                colors.accent.with_alpha(0.16)
                            } else {
                                Color::TRANSPARENT
                            })
                            .padding(7.dp()),
                        self.choice.action(move |current| *current = choice),
                    );
                }
            },
        );
    }

    fn icon(&self, choice: ThemeChoice) -> &'static Icon {
        match choice {
            ThemeChoice::System => self.system,
            ThemeChoice::Light => self.light,
            ThemeChoice::Dark => self.dark,
        }
    }
}

#[cfg(test)]
mod tests;
