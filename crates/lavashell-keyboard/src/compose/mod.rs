// SPDX-License-Identifier: GPL-3.0-only
mod area;
mod key;
mod overlay;

use crate::board::{FUSE_DISTANCE, GAP, Grid, KeyAt, PADDING, row_numbers};
use crate::keyboard::{Keyboard, Snapshot};
use crate::look::{self, Font, Look};
use crate::popup::Popup;
use crate::stage::{self, Opening, Stage};
use crate::style::KEYBOARD_STYLE;
use area::Area;
use lavashell_core::prelude::*;
use lavashell_core::ui::{Bounds, TransformHandler};
use std::hash::Hash;

pub trait UiKeyboard {
    /// Composes `keyboard` as wide as the viewport and as tall as the part of the keyboard panel
    /// that is inside the view, with the space below a docked panel. The panel rises from below the
    /// view and sinks back. The height of the keyboard follows the panel in every frame, so the
    /// content of a column above the keyboard ends at the top of the panel. The clock of this view
    /// runs the timers started by the keyboard. A keyboard with the target `Target::View` types
    /// into the focused field of this view. `modifier` is applied to the root node of the keyboard
    /// after the size of the keyboard.
    fn keyboard(&mut self, keyboard: &Keyboard, modifier: Modifier);

    /// Composes `keyboard` like `UiKeyboard::keyboard`, but shares the glass of the panel with a
    /// node of the host under the key `origin`. The host composes that node with `ui.shared(origin,
    /// ..)` while `Keyboard::shown` is false, such as an island of a taskbar. The glass grows out
    /// of that node when the keyboard shows and shrinks back into it when the keyboard hides.
    /// `closed` is the height of the keyboard while it is hidden, which is the distance from the
    /// top of that node to the bottom of the keyboard. The height of the keyboard moves between
    /// `closed` and the height of the open keyboard (the panel and the space below a docked panel)
    /// together with the glass, so the content of a column above the keyboard ends at the glass in
    /// every frame.
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

/// Composes the keyboard with the height `closed` while it is hidden and the height of its area
/// while it is open. In between, the height follows the opening motion of the panel. The height is
/// set on a node around the scoped component of the keyboard, so the layout and the painting of the
/// keyboard do not change while the height moves. A change of the placement moves the height of
/// the open keyboard on the show spring, with the glass. The keyboard records the height that the
/// layout measures for `Keyboard::height`.
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
        .map(|((closed, open), rest)| stage::between(closed, rest, open));
    let lift = entrance.lift(&height, size.height);
    let _current_root = ui.state_of(ui.next_id(), "root", || keyboard.root());
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
    /// The glass grows out of the node that the host shares under the key, and shrinks back into
    /// it.
    From(K),
}

impl<K: Hash> Entrance<K> {
    fn glass(&self, ui: &mut Ui, modifier: Modifier, rows: impl FnOnce(&mut Ui)) {
        match self {
            Self::Below => ui.layout(Arrangement::Overlay, modifier, rows),
            Self::From(origin) => ui.shared(origin, modifier, rows),
        }
    }

    /// The distance from the top of a keyboard that is `height` tall to the top of the area that
    /// contains the panel, which is `rest` tall. It is `None` for an area that starts at the top of
    /// the keyboard. An area whose glass is shared with a node of the host ends at the bottom of the
    /// view, because the glass grows from that node to there.
    fn lift(&self, height: &Deferred<Dp>, rest: Dp) -> Option<Deferred<Dp, Bounds>> {
        match self {
            Self::Below => None,
            Self::From(_) => Some(height.map(move |height| height - rest).into()),
        }
    }
}

/// Composes the panel at the stage recorded before this composition advances the stage. The host
/// reads the stage through `Keyboard::shown`, so the host and the keyboard compose the next stage
/// in the same frame. A panel that enters from below shows the fused rows until it has sunk out of
/// the view. The previews and the popup are measured against the screen, so they can be taller than
/// the area.
fn compose<K: Hash>(ui: &mut Ui, keyboard: &Keyboard, entrance: &Entrance<K>, area: Area) {
    keyboard.follow_platform(ui);
    let snapshot = keyboard.snapshot();
    let stage = keyboard.stages().get();
    let panel = area.panel;
    let pixel = ui.viewport().pixel() / panel.height;
    let advanced = keyboard.advance_stage(
        snapshot.is_some(),
        ui.local(&REDUCED_MOTION),
        panel.height,
        pixel,
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
    let look = looks.get(ui.local(&REDUCED_TRANSPARENCY));
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
            let popup = (snapshot.as_ref()).and_then(|snapshot| snapshot.popup.as_ref());
            let transform =
                (keyboard.transform_handler()).filter(|_| !popup.is_some_and(Popup::under_finger));
            match transform {
                Some(handler) => surface(keyboard).then(passes_transforms(handler)),
                None => surface(keyboard),
            }
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

fn on_press(keyboard: &Keyboard, action: fn(&Keyboard)) -> Modifier {
    let keyboard = keyboard.clone();
    Modifier::new().on_pointer(move |input| {
        if input.phase == PointerPhase::Pressed {
            action(&keyboard);
        }
    })
}

/// The modifier of the glass of the panel. A press on the panel closes a list of layouts that is
/// still open and does not take the focus from the field.
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

/// Passes each transform of two fingers on the glass of the panel to `handler`. The transform takes
/// the fingers from the keys.
fn passes_transforms(handler: TransformHandler) -> Modifier {
    Modifier::new().on_transform(move |input| handler.borrow_mut()(input))
}

/// Composes the glass of the panel around `rows`, with `surface` applied after its shape. The glass
/// clips its content, so the previews and the popup, which extend above the glass, are composed
/// outside it. The glass moves to a new placement on the show spring, with its corners. When the
/// glass is shared with a node of the host, the glass grows out of that node on the show spring and
/// the rows move with it.
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

/// Composes the rows of keys. Each row paints its keys as one material group. The keys enter
/// enlarged, so they start as one merged shape and split apart.
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

/// The fade between the rows of keys and the fused rows.
fn fading() -> Transition {
    Transition {
        bounds: BoundsMotion::Instant,
        ..Transition::new(stage::fuse_tween())
    }
}
