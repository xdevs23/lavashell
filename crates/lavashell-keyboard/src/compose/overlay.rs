// SPDX-License-Identifier: GPL-3.0-only
use super::{Scene, centered, on_press, space};
use crate::board::{FUSE_DISTANCE, at_both_ends};
use crate::keyboard::Keyboard;
use crate::layouts::Layout;
use crate::look::{
    self, ARRANGEMENT, BUBBLE_CORNERS, BUBBLE_HEIGHT, BUBBLE_OVERHANG, CHECK_ICON, ENTRY_CORNERS,
    ENTRY_PADDING, HEADING_INDENT, KEY_CORNERS, LAYOUT_NAME, LETTER, LIST_CORNERS, LIST_HEADING,
    NAME_GAP, POPUP_CORNERS, PREVIEW,
};
use crate::popup::{Entries, HEADING_HEIGHT, LIST_PADDING, Placed};
use crate::press::Preview;
use lavashell_core::prelude::*;

pub(super) fn previews(ui: &mut Ui, scene: &Scene) {
    let snapshot = scene.snapshot;
    if !snapshot.platform.preferences.key_preview || snapshot.session.text == FieldText::Protected {
        return;
    }
    for preview in &snapshot.previews {
        if snapshot.board.key(preview.at) == Some(preview.key) {
            compose(ui, scene, preview);
        }
    }
}

fn compose(ui: &mut Ui, scene: &Scene, preview: &Preview) {
    let key = scene.grid.rect(&scene.snapshot.board, preview.at);
    let size = Size::new(
        key.width + at_both_ends(BUBBLE_OVERHANG),
        key.height + BUBBLE_HEIGHT,
    );
    let x = scene.grid.fit_left(key.x - BUBBLE_OVERHANG, size.width);
    let y = key.y + key.height - size.height;
    let [opened, closed] = [look::PREVIEW_OPENED_FROM, look::PREVIEW_CLOSED_AT].map(|scale| {
        Presence::default()
            .scale(scale)
            .scale_origin(ScaleOrigin::BOTTOM)
    });
    let style = scene.text(&PREVIEW, scene.look.ink);
    ui.key(
        ("preview", preview.serial),
        Modifier::new()
            .out_of_flow()
            .semantics(Semantics {
                mode: SemanticsMode::Hidden,
                ..Semantics::default()
            })
            .offset(x, y)
            .size(size.width, size.height)
            .material_group(scene.look.preview.clone(), FUSE_DISTANCE)
            .transition(
                Transition::new(look::preview_opening())
                    .enter(opened)
                    .exit_with(closed, look::preview_closing()),
            ),
        |ui| {
            ui.spacer(
                size.width,
                BUBBLE_HEIGHT,
                Modifier::new().island(BUBBLE_CORNERS),
            );
            ui.spacer(
                key.width,
                key.height,
                Modifier::new()
                    .offset(key.x - x, BUBBLE_HEIGHT)
                    .island(KEY_CORNERS),
            );
            centered(ui, Modifier::new().size(size.width, BUBBLE_HEIGHT), |ui| {
                ui.text(&preview.label, Modifier::new().text_style(style));
            });
        },
    );
}

pub(super) fn popup(ui: &mut Ui, scene: &Scene) {
    let snapshot = scene.snapshot;
    let Some(popup) = &snapshot.popup else {
        return;
    };
    let Some(placed) = Placed::new(popup, snapshot, scene.grid) else {
        return;
    };
    let frame = placed.frame;
    let text = &snapshot.settings.text;
    let (name, corners) = match placed.entries {
        Entries::FurtherLetters(_) => (text.more_letters.clone(), POPUP_CORNERS),
        Entries::Layouts(_) => (text.layouts.clone(), LIST_CORNERS),
    };
    let modifier = on_press(scene.keyboard, Keyboard::type_held)
        .out_of_flow()
        .focusable(false)
        .semantics(Semantics {
            role: Some(Role::Menu),
            name: Some(name),
            ..Semantics::default()
        })
        .offset(frame.x, frame.y)
        .size(frame.width, frame.height)
        .material(scene.look.popup.clone(), corners)
        .transition(Transition::new(look::show_spring()).reflow());
    ui.key("popup", modifier, |ui| {
        if matches!(placed.entries, Entries::Layouts(_)) {
            let style = scene.text(&LIST_HEADING, scene.look.muted);
            ui.layout(
                Axis::Horizontal,
                Modifier::new()
                    .align_items(Align::Center)
                    .offset(LIST_PADDING + HEADING_INDENT, LIST_PADDING)
                    .height(HEADING_HEIGHT),
                |ui| {
                    ui.text(&text.layouts, Modifier::new().text_style(style));
                },
            );
        }
        for index in 0..placed.entries.len() {
            entry(ui, scene, &placed, popup.chosen, index);
        }
    });
}

fn entry(ui: &mut Ui, scene: &Scene, placed: &Placed, chosen: Option<usize>, index: usize) {
    let Some(rect) = placed.entry(index) else {
        return;
    };
    let chosen_fill = (chosen == Some(index)).then_some(scene.look.chosen);
    let position = Modifier::new()
        .offset(rect.x - placed.frame.x, rect.y - placed.frame.y)
        .size(rect.width, rect.height);
    match &placed.entries {
        Entries::FurtherLetters(letters) => {
            if let Some(letter) = letters.get(index) {
                let fill = chosen_fill.unwrap_or(scene.look.entry);
                further_letter_entry(ui, scene, position, fill, letter, index);
            }
        }
        Entries::Layouts(layouts) => {
            if let Some(layout) = layouts.get(index) {
                layout_entry(ui, scene, position, chosen_fill, layout, index);
            }
        }
    }
}

fn entry_content(modifier: Modifier, semantics: Semantics) -> Modifier {
    modifier
        .padding_xy(ENTRY_PADDING, Dp::ZERO)
        .align_items(Align::Center)
        .focusable(false)
        .semantics(semantics)
}

fn further_letter_entry(
    ui: &mut Ui,
    scene: &Scene,
    position: Modifier,
    fill: Color,
    letter: char,
    index: usize,
) {
    let picker = scene.keyboard.clone();
    let letter = scene.snapshot.cased(letter).to_string();
    let style = scene.text(&LETTER, scene.look.ink);
    let semantics = Semantics {
        role: Some(Role::MenuItem),
        name: Some(letter.clone()),
        ..Semantics::default()
    };
    let modifier = position
        .material(Material::new(fill), ENTRY_CORNERS)
        .on_accessibility_action(ActionKind::Activate, move |_| {
            picker.pick(index);
            Ok(())
        });
    centered(ui, entry_content(modifier, semantics), |ui| {
        ui.text(letter, Modifier::new().text_style(style));
    });
}

fn layout_entry(
    ui: &mut Ui,
    scene: &Scene,
    position: Modifier,
    chosen_fill: Option<Color>,
    layout: &Layout,
    index: usize,
) {
    let look = scene.look;
    let picker = scene.keyboard.clone();
    let current = layout == scene.snapshot.layout;
    let mut semantics = Semantics {
        role: Some(Role::MenuItem),
        name: Some(layout.name().to_string()),
        ..Semantics::default()
    };
    semantics.state.checked = Some(if current {
        Checked::True
    } else {
        Checked::False
    });
    let fill = chosen_fill.or_else(|| current.then_some(look.entry));
    let modifier = match fill {
        Some(fill) => position.material(Material::new(fill), KEY_CORNERS),
        None => position,
    };
    let row = Modifier::new()
        .gap(NAME_GAP)
        .then(on_press(scene.keyboard, Keyboard::type_held))
        .on_click(move || picker.pick(index));
    ui.layout(
        Axis::Horizontal,
        row.then(entry_content(modifier, semantics)),
        |ui| {
            ui.text(
                layout.name(),
                Modifier::new().text_style(scene.text(&LAYOUT_NAME, look.ink)),
            );
            ui.text(
                layout.arrangement(),
                Modifier::new().text_style(scene.text(&ARRANGEMENT, look.muted)),
            );
            space(ui);
            if current {
                ui.icon(
                    scene.snapshot.settings.icons.active_layout,
                    IconOptions {
                        size: Some(CHECK_ICON),
                        ..IconOptions::default()
                    },
                    Modifier::new().color(look.accent),
                );
            }
        },
    );
}
