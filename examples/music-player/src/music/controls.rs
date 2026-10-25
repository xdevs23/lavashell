// SPDX-License-Identifier: GPL-3.0-only
use super::{CopyKey, Model, Section, copy};
use crate::theme::{ACCENT, ACTIVE, LINE, MUTED, SIDE, TEXT, title};
use lavashell::prelude::*;

pub(super) fn text(ui: &mut Ui, value: impl Into<String>, size: Sp, color: Color) {
    ui.text(value, Modifier::new().font_size(size).color(color));
}

pub(super) fn heading(ui: &mut Ui, value: impl Into<String>, size: Sp) {
    ui.text(value, Modifier::new().text_style(title(size)));
}

pub(super) fn line(ui: &mut Ui) {
    ui.spacer(
        0.dp(),
        1.dp(),
        Modifier::new().fill_max_width().background(LINE),
    );
}

/// A button of the player that shows an icon.
#[derive(Clone, Copy)]
pub(super) enum Control {
    SaveCurrent,
    Shuffle,
    Previous,
    Next,
    Repeat,
}

impl Control {
    fn tag(self) -> &'static str {
        match self {
            Self::SaveCurrent => "save-current",
            Self::Shuffle => "shuffle",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Repeat => "repeat",
        }
    }

    fn name(self, selected: bool) -> CopyKey {
        match self {
            Self::SaveCurrent if selected => CopyKey::Saved,
            Self::SaveCurrent => CopyKey::Save,
            Self::Shuffle => CopyKey::Shuffle,
            Self::Previous => CopyKey::Previous,
            Self::Next => CopyKey::Next,
            Self::Repeat => CopyKey::Repeat,
        }
    }

    fn is_switch(self) -> bool {
        matches!(self, Self::SaveCurrent | Self::Shuffle | Self::Repeat)
    }
}

pub(super) fn button(
    ui: &mut Ui,
    icon: &'static Icon,
    control: Control,
    selected: bool,
    model: &State<Model>,
    action: impl Fn(&mut Model) + 'static,
) {
    let name = copy(control.name(selected));
    let switch = control.is_switch();
    ui.icon_button(
        icon,
        IconOptions {
            size: Some(19.dp()),
            ..IconOptions::default()
        },
        Modifier::new()
            .color(if selected { ACCENT } else { MUTED })
            .tag(control.tag())
            .accessible_name(name)
            .tooltip(name)
            .semantics_with(move |s| {
                if switch {
                    s.role = Some(Role::Switch);
                    s.state.checked = Some(if selected {
                        Checked::True
                    } else {
                        Checked::False
                    });
                }
            })
            .clip(8.dp())
            .padding(9.dp()),
        model.action(action),
    );
}

pub(super) fn nav(
    ui: &mut Ui,
    model: &State<Model>,
    section: Section,
    icon: &'static Icon,
    key: CopyKey,
    tag: &str,
) {
    let selected = model.get().section == section;
    ui.row(
        Modifier::new()
            .gap(12.dp())
            .align_items(Align::Center)
            .tag(tag)
            .accessible_name(copy(key))
            .selected(selected)
            .on_click(model.action(move |m| {
                m.section = section;
                m.album = None;
            }))
            .fill_max_width()
            .clip(8.dp())
            .background(if selected { ACTIVE } else { SIDE })
            .padding_xy(12.dp(), 11.dp()),
        |ui| {
            ui.icon(
                icon,
                IconOptions {
                    size: Some(19.dp()),
                    ..IconOptions::default()
                },
                Modifier::new().color(if selected { ACCENT } else { MUTED }),
            );
            text(ui, copy(key), 14.sp(), if selected { TEXT } else { MUTED });
        },
    );
}
