// SPDX-License-Identifier: GPL-3.0-only
use super::{Scene, centered, on_press, space};
use crate::board::{FUSE_DISTANCE, at_both_ends};
use crate::keyboard::Keyboard;
use crate::look::{
    self, ARRANGEMENT, BUBBLE_CORNERS, BUBBLE_HEIGHT, BUBBLE_OVERHANG, CHECK_ICON, ENTRY_CORNERS,
    ENTRY_PADDING, HEADING_INDENT, KEY_CORNERS, LAYOUT_NAME, LETTER, LIST_CORNERS, LIST_HEADING,
    NAME_GAP, POPUP_CORNERS, PREVIEW,
};
use crate::popup::{Entries, HEADING_HEIGHT, LIST_PADDING, Placed};
use crate::press::Preview;
use lavashell_core::prelude::*;

/// Composes a preview bubble for each pressed key, the later previews on top of the earlier ones. A
/// preview is composed only while its key is still at the same place on the board and the key
/// preview preference is on, and never in a protected field.
pub(super) fn previews(ui: &mut Ui, scene: &Scene) {
    let snapshot = scene.snapshot;
    if !snapshot.facts.preferences.key_preview || snapshot.session.text == FieldText::Protected {
        return;
    }
    for preview in &snapshot.previews {
        if snapshot.board.key(preview.at) == Some(preview.key) {
            compose(ui, scene, preview);
        }
    }
}

/// Composes the preview of one key: a bubble with the character above the key, merged with a copy
/// of the key into one shape. The preview scales about the middle of its bottom edge as it opens
/// and closes.
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
    let tween = if preview.released {
        look::preview_closing()
    } else {
        look::preview_opening()
    };
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
            .transition(Transition::new(tween).enter(opened).exit(closed)),
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

/// Composes the open popup above the keys. A press on the popup types the held keys and is not
/// passed to the keys below the popup.
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
        .material(scene.look.popup.clone(), corners);
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
            entry(ui, scene, &placed, popup.chosen == Some(index), index);
        }
    });
}

/// Composes one entry of the popup. An entry is picked when the finger that opened the popup is
/// lifted over it. An entry in the list of layouts is also picked by a tap that ends on it.
fn entry(ui: &mut Ui, scene: &Scene, placed: &Placed, chosen: bool, index: usize) {
    let look = scene.look;
    let Some(rect) = placed.entry(index) else {
        return;
    };
    let picker = scene.keyboard.clone();
    let mut semantics = Semantics {
        role: Some(Role::MenuItem),
        ..Semantics::default()
    };
    let fill = if chosen { look.chosen } else { look.entry };
    let mut modifier = Modifier::new()
        .offset(rect.x - placed.frame.x, rect.y - placed.frame.y)
        .size(rect.width, rect.height);
    let entry_modifier = |modifier: Modifier, semantics: Semantics| {
        modifier
            .padding_xy(ENTRY_PADDING, Dp::ZERO)
            .align_items(Align::Center)
            .focusable(false)
            .semantics(semantics)
    };
    match &placed.entries {
        Entries::FurtherLetters(letters) => {
            let Some(letter) = letters.get(index) else {
                return;
            };
            let letter = scene.snapshot.cased(*letter).to_string();
            let style = scene.text(&LETTER, look.ink);
            semantics.name = Some(letter.clone());
            let modifier = modifier
                .material(Material::new(fill), ENTRY_CORNERS)
                .on_accessibility_action(ActionKind::Activate, move |_| {
                    picker.pick(index);
                    Ok(())
                });
            centered(ui, entry_modifier(modifier, semantics), |ui| {
                ui.text(letter, Modifier::new().text_style(style));
            });
        }
        Entries::Layouts(layouts) => {
            let Some(layout) = layouts.get(index) else {
                return;
            };
            let current = layout.id() == scene.snapshot.layout.id();
            semantics.name = Some(layout.name().to_string());
            semantics.state.checked = Some(if current {
                Checked::True
            } else {
                Checked::False
            });
            if chosen || current {
                modifier = modifier.material(Material::new(fill), KEY_CORNERS);
            }
            let row = Modifier::new()
                .gap(NAME_GAP)
                .then(on_press(scene.keyboard, Keyboard::type_held))
                .on_click(move || picker.pick(index));
            ui.layout(
                Axis::Horizontal,
                row.then(entry_modifier(modifier, semantics)),
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
    }
}
