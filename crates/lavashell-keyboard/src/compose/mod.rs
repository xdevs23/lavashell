// SPDX-License-Identifier: GPL-3.0-only
mod area;
mod key;
mod overlay;

use crate::board::{FUSE_DISTANCE, GAP, Grid, KeyAt, PADDING, row_numbers};
use crate::keyboard::{Keyboard, Snapshot};
use crate::look::{self, Font, Look, Surface};
use crate::popup::Popup;
use crate::stage::{self, MotionPreference, Opening, Stage};
use crate::style::KEYBOARD_STYLE;
use area::Area;
use lavashell_core::prelude::*;
use lavashell_core::ui::{Bounds, TransformHandler};
use std::hash::Hash;

pub trait UiKeyboard {
    fn keyboard(&mut self, keyboard: &Keyboard, modifier: Modifier);

    fn keyboard_from<K: Hash + 'static>(
        &mut self,
        keyboard: &Keyboard,
        origin: K,
        closed: impl Into<Deferred<Dp>>,
        modifier: Modifier,
    );
}

impl UiKeyboard for Ui {
    fn keyboard(&mut self, keyboard: &Keyboard, modifier: Modifier) {
        keyboard_node(
            self,
            keyboard,
            Entrance::<()>::Below,
            &Dp::ZERO.into(),
            modifier,
        );
    }

    fn keyboard_from<K: Hash + 'static>(
        &mut self,
        keyboard: &Keyboard,
        origin: K,
        closed: impl Into<Deferred<Dp>>,
        modifier: Modifier,
    ) {
        keyboard_node(
            self,
            keyboard,
            Entrance::From(origin),
            &closed.into(),
            modifier,
        );
    }
}

fn keyboard_node<K: Hash + 'static>(
    ui: &mut Ui,
    keyboard: &Keyboard,
    entrance: Entrance<K>,
    closed: &Deferred<Dp>,
    modifier: Modifier,
) {
    if let Err(error) = keyboard.compose_in(ui) {
        ui.fail(error);
        return;
    }
    let panel_height = keyboard.settings().panel_height.get();
    let area = Area::new(keyboard.placement(), ui.viewport().size, panel_height);
    let size = area.size;
    let rest = match keyboard.stages().animate_rest_to(size.height) {
        Ok(rest) => Deferred::<Dp>::from(&rest),
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let open = Deferred::<Opening>::from(&keyboard.stages().open()).map(Opening::share);
    let height = (closed.zip(&open).zip(&rest))
        .map(|((closed, open), rest)| stage::interpolated(closed, rest, open));
    let lift = entrance.lift(&height, size.height);
    let _root_state_is_created_for_later_frames =
        ui.state_of(ui.next_id(), "root", || keyboard.root());
    let (keyboard, recording) = (keyboard.clone(), keyboard.clone());
    let scoped = move |ui: &mut Ui| {
        ui.scoped("keyboard", Modifier::new(), move |ui| {
            compose(ui, &keyboard, &entrance, area);
        });
    };
    let root = Modifier::new()
        .width(size.width)
        .height(height)
        .measurement_limit(size)
        .then(modifier)
        .on_measure(move |size| recording.record_height(size.height));
    ui.layout(Arrangement::Overlay, root, |ui| match lift {
        Some(lift) => {
            ui.layout(
                Arrangement::Overlay,
                Modifier::new().offset(Dp::ZERO, lift),
                scoped,
            );
        }
        None => scoped(ui),
    });
}

struct Scene<'a> {
    keyboard: &'a Keyboard,
    snapshot: &'a Snapshot,
    look: &'a Look,
    grid: Grid,
    family: String,
}

impl Scene<'_> {
    fn text(&self, font: &Font, color: Color) -> TextStyle {
        TextStyle::new(self.family.clone(), font.size, color).weight(font.weight)
    }
}

fn space(ui: &mut Ui) {
    ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
}

fn centered(ui: &mut Ui, modifier: Modifier, content: impl FnOnce(&mut Ui)) {
    ui.layout(
        Axis::Horizontal,
        Modifier::new().align_items(Align::Center).then(modifier),
        |ui| {
            space(ui);
            content(ui);
            space(ui);
        },
    );
}

enum Entrance<K> {
    Below,
    From(K),
}

impl<K: Hash> Entrance<K> {
    fn glass(&self, ui: &mut Ui, modifier: Modifier, rows: impl FnOnce(&mut Ui)) {
        match self {
            Self::Below => ui.layout(Arrangement::Overlay, modifier, rows),
            Self::From(origin) => ui.shared(origin, modifier, rows),
        }
    }

    fn lift(&self, height: &Deferred<Dp>, rest: Dp) -> Option<Deferred<Dp, Bounds>> {
        match self {
            Self::Below => None,
            Self::From(_) => Some(height.map(move |height| height - rest).into()),
        }
    }
}

fn compose<K: Hash>(ui: &mut Ui, keyboard: &Keyboard, entrance: &Entrance<K>, area: Area) {
    keyboard.follow_platform(ui);
    let snapshot = keyboard.snapshot();
    let stage = keyboard.stages().get();
    let panel = area.panel;
    let pixel_share = ui.viewport().pixel() / panel.height;
    let advanced = keyboard.advance_stage(
        snapshot.is_some(),
        motion_preference(ui),
        panel.height,
        pixel_share,
    );
    if let Err(error) = advanced {
        ui.fail(error);
        return;
    }
    let sinking = stage == Stage::Hidden
        && matches!(entrance, Entrance::Below)
        && !keyboard.stages().open().is_finished();
    if stage == Stage::Hidden && !sinking {
        return;
    }
    let key_height = Grid::key_height_in(panel.height);
    let looks = ui.local(&KEYBOARD_STYLE).looks(key_height);
    let look = looks.get(if ui.local(&REDUCED_TRANSPARENCY) {
        Surface::Opaque
    } else {
        Surface::Glass
    });
    let family = ui.local(&TEXT_STYLE).family;
    ui.provide(&INTERACTION_STYLE, InteractionStyle::default(), |ui| {
        let scene = (snapshot.as_ref()).map(|snapshot| Scene {
            keyboard,
            snapshot,
            look,
            grid: Grid::new(panel, area.screen, snapshot.board.columns, key_height),
            family,
        });
        let open = scene.as_ref().filter(|_| stage == Stage::Open);
        let panel_surface = if sinking {
            Modifier::new()
        } else {
            surface_taking_transforms(keyboard, snapshot.as_ref())
        };
        let content = Modifier::new()
            .size(area.size.width, area.size.height)
            .measurement_limit(area.screen.size());
        ui.layout(Arrangement::Overlay, content, |ui| {
            glass(ui, look, &area, entrance, panel_surface, |ui| match open {
                Some(scene) => rows(ui, scene),
                None if stage.fused() || sinking => {
                    fused(ui, look, panel.width, key_height);
                }
                None => {}
            });
            if let Some(scene) = open {
                overlay::previews(ui, scene);
                overlay::popup(ui, scene);
            }
        });
    });
}

fn motion_preference(ui: &Ui) -> MotionPreference {
    if ui.local(&REDUCED_MOTION) {
        MotionPreference::Reduced
    } else {
        MotionPreference::Full
    }
}

fn surface_taking_transforms(keyboard: &Keyboard, snapshot: Option<&Snapshot>) -> Modifier {
    let popup = snapshot.and_then(|snapshot| snapshot.popup.as_ref());
    let transform =
        (keyboard.transform_handler()).filter(|_| !popup.is_some_and(Popup::under_finger));
    match transform {
        Some(handler) => surface(keyboard).then(passes_transforms(handler)),
        None => surface(keyboard),
    }
}

fn on_press(keyboard: &Keyboard, action: fn(&Keyboard)) -> Modifier {
    let keyboard = keyboard.clone();
    Modifier::new().on_pointer(move |input| {
        if input.phase == PointerPhase::Pressed {
            action(&keyboard);
        }
    })
}

fn surface(keyboard: &Keyboard) -> Modifier {
    let name = keyboard.settings().text.keyboard.clone();
    on_press(keyboard, Keyboard::begin_press)
        .focusable(false)
        .semantics(Semantics {
            role: Some(Role::Group),
            name: Some(name),
            ..Semantics::default()
        })
}

fn passes_transforms(handler: TransformHandler) -> Modifier {
    Modifier::new().on_transform(move |input| handler.borrow_mut()(input))
}

fn glass<K: Hash>(
    ui: &mut Ui,
    look: &Look,
    area: &Area,
    entrance: &Entrance<K>,
    surface: Modifier,
    rows: impl FnOnce(&mut Ui),
) {
    let (panel, corners) = (area.panel, area.corners);
    let modifier = Modifier::new()
        .offset(panel.x, panel.y)
        .size(panel.width, panel.height)
        .material(look.panel.clone(), corners)
        .clip(corners)
        .transition(Transition::new(look::show_spring()).reflow());
    entrance.glass(ui, modifier.then(surface), rows);
}

fn rows(ui: &mut Ui, scene: &Scene) {
    ui.layout(
        Axis::Vertical,
        Modifier::new()
            .gap(GAP)
            .insets(PADDING)
            .transition(fading().exit(Presence::fade())),
        |ui| {
            for (row, slots) in scene.snapshot.board.rows.iter().enumerate() {
                ui.layout(
                    Axis::Horizontal,
                    Modifier::new()
                        .gap(GAP)
                        .material_group(scene.look.keys.clone(), FUSE_DISTANCE),
                    |ui| {
                        for (slot, entry) in slots.iter().enumerate() {
                            match entry.key {
                                Some(key) => key::compose(ui, scene, KeyAt { row, slot }, key),
                                None => {
                                    ui.spacer(
                                        scene.grid.width(entry.span),
                                        scene.grid.key_height(),
                                        Modifier::new(),
                                    );
                                }
                            }
                        }
                    },
                );
            }
        },
    );
}

fn fused(ui: &mut Ui, look: &Look, width: Dp, height: Dp) {
    let corners = look::pill(height);
    ui.key(
        "fused",
        Modifier::new().transition(fading().enter(Presence::fade()).exit(Presence::fade())),
        |ui| {
            ui.layout(
                Axis::Vertical,
                Modifier::new().gap(GAP).insets(PADDING),
                |ui| {
                    for _ in row_numbers() {
                        let width = (width - PADDING.left - PADDING.right).max(Dp::ZERO);
                        ui.spacer(
                            width,
                            height,
                            Modifier::new().material(look.keys.clone(), corners),
                        );
                    }
                },
            );
        },
    );
}

fn fading() -> Transition {
    Transition {
        bounds: BoundsMotion::Instant,
        ..Transition::new(stage::fuse_tween())
    }
}
