// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, copy,
    look::{self, Palette},
    model::{Demo, PANEL_HEIGHT, SceneIcons, TRACKS, Track},
};
use lavashell::keyboard::{IslandReturn, KEYBOARD_STYLE, UiKeyboard};
use lavashell::prelude::*;
use lavashell_example_support::cover;
use std::time::Duration;

/// The pixel density of the car screen in the sample, in dots per inch.
const SCREEN_DPI: f64 = 213.0;

/// The `Density` of the car screen, made from `SCREEN_DPI`.
#[must_use]
pub fn density() -> Density {
    constant!(Density::from_dpi(SCREEN_DPI))
}

/// The car screen of the sample, 1380 by 620 pixels.
#[must_use]
pub fn screen() -> Size {
    Size::from_px(1380.px(), 620.px(), density())
}

pub const FIELD: &str = "search";
/// The space around the text of the search field, which leaves room for its icons.
pub const FIELD_TEXT: Insets = Insets {
    top: Dp::ZERO,
    right: Dp::new(68.0),
    bottom: Dp::ZERO,
    left: Dp::new(60.0),
};
const MARGIN: Dp = Dp::new(12.0);
const TASKBAR: Dp = Dp::new(64.0);
const BELOW_WINDOW: Dp = Dp::new(88.0);
const FIELD_LEFT: Dp = Dp::new(78.0);
const FIELD_HEIGHT: Dp = Dp::new(64.0);
const RESULTS_TOP: Dp = Dp::new(92.0);
const DISC: Dp = Dp::new(40.0);
const DISC_ICON: Dp = Dp::new(22.0);
const TASKBAR_GLASS: &str = "taskbar-glass";

/// The border of a surface, one dp wide. Each surface sets the color of the border from the edge color of the
/// palette for that surface.
const EDGE: Stroke = constant!(Stroke::new(Color::TRANSPARENT, Dp::new(1.0)));

pub fn compose(ui: &mut Ui, demo: &Demo) {
    let appearance = demo.appearance.get();
    let palette = appearance.palette();
    let size = ui.viewport().size;
    let style = appearance.keyboard_style();
    ui.provide(&TEXT_STYLE, look::text(palette.ink, 16.sp()), |ui| {
        ui.provide(&INTERACTION_STYLE, look::interaction(palette), |ui| {
            ui.provide(&KEYBOARD_STYLE, style, |ui| {
                ui.stack(Modifier::new().fill_max_width().fill_max_height(), |ui| {
                    let backdrop = look::backdrop(palette, size);
                    ui.spacer(
                        size.width,
                        size.height,
                        Modifier::new().background(backdrop),
                    );
                    let keyboard = NodeRef::new();
                    window(ui, demo, palette, size, &keyboard.height());
                    if !demo.keyboard.shown() {
                        taskbar(ui, palette, demo.icons, size);
                    }
                    ui.column(Modifier::new().fill_max_width().fill_max_height(), |ui| {
                        ui.space(Modifier::new());
                        ui.keyboard_from(
                            &demo.keyboard,
                            TASKBAR_GLASS,
                            TASKBAR + MARGIN,
                            Modifier::new().node_ref(&keyboard),
                        );
                    });
                });
            });
        });
    });
}

fn window(ui: &mut Ui, demo: &Demo, palette: &Palette, size: Size, keyboard_height: &Deferred<Dp>) {
    let width = size.width - MARGIN * 2.0;
    let height =
        keyboard_height.map(move |keyboard| size.height - MARGIN - keyboard.max(BELOW_WINDOW));
    let corners = 20.dp();
    let surface = Material::new(palette.window).border(Stroke {
        color: palette.window_edge,
        ..EDGE
    });
    ui.stack(
        Modifier::new()
            .offset(MARGIN, MARGIN)
            .size(width, height)
            .material(surface, corners)
            .clip(corners),
        |ui| {
            back(ui, demo, palette);
            field(ui, demo, palette, width - FIELD_LEFT - MARGIN);
            ui.column(
                Modifier::new().offset(Dp::ZERO, RESULTS_TOP).width(width),
                |ui| {
                    for track in &TRACKS {
                        result(ui, palette, track);
                    }
                },
            );
        },
    );
}

fn back(ui: &mut Ui, demo: &Demo, palette: &Palette) {
    let (query, ink, back) = (demo.query.clone(), palette.ink, demo.icons.back);
    ui.button(
        Modifier::new()
            .accessible_name(copy(CopyKey::Back))
            .offset(10.dp(), 16.dp())
            .material(Material::new(palette.field), 28.dp())
            .padding(15.dp()),
        move || query.set(String::new()),
        |ui| {
            ui.icon(
                back,
                IconOptions {
                    size: Some(26.dp()),
                    ..IconOptions::default()
                },
                Modifier::new().color(ink),
            );
        },
    );
}

fn field(ui: &mut Ui, demo: &Demo, palette: &Palette, width: Dp) {
    let (query, action) = (demo.query.get(), demo.action.get());
    let (change, clear) = (demo.query.clone(), demo.query.clone());
    let pill = FIELD_HEIGHT / 2.0;
    ui.stack(
        Modifier::new()
            .focus_within(BorderStyle {
                color: palette.app,
                width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                radius: pill,
            })
            .offset(FIELD_LEFT, 12.dp()),
        |ui| {
            ui.text_field(
                query.clone(),
                TextFieldOptions {
                    placeholder: copy(CopyKey::Search).into(),
                    hints: TextHints {
                        action: Some(action),
                        ..TextHints::default()
                    },
                },
                Modifier::new()
                    .accessible_name(copy(CopyKey::Search))
                    .tag(FIELD)
                    .text_style(look::text(palette.ink, 23.sp()))
                    .size(width, FIELD_HEIGHT)
                    .material(Material::new(palette.field), pill)
                    .insets(FIELD_TEXT),
                move |text| change.set(text),
            );
            ui.icon(
                demo.icons.search,
                IconOptions {
                    size: Some(26.dp()),
                    ..IconOptions::default()
                },
                Modifier::new()
                    .color(palette.muted)
                    .offset(20.dp(), 19.dp()),
            );
            if !query.is_empty() {
                let (ink, cross) = (palette.ink, demo.icons.clear);
                ui.button(
                    Modifier::new()
                        .focusable(false)
                        .accessible_name(copy(CopyKey::Clear))
                        .offset(width - 54.dp(), 10.dp())
                        .material(Material::new(palette.clear), 22.dp())
                        .padding(11.dp()),
                    move || clear.set(String::new()),
                    |ui| {
                        ui.icon(
                            cross,
                            IconOptions {
                                size: Some(22.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(ink),
                        );
                    },
                );
            }
        },
    );
}

fn result(ui: &mut Ui, palette: &Palette, track: &'static Track) {
    let hue = track.hue;
    ui.row(
        Modifier::new()
            .gap(16.dp())
            .align_items(Align::Center)
            .fill_max_width()
            .height(72.dp())
            .padding_xy(20.dp(), Dp::ZERO),
        |ui| {
            ui.canvas(Size::new(52.dp(), 52.dp()), Modifier::new(), move |d| {
                cover(d, hue);
            });
            ui.column(Modifier::new().gap(2.dp()), |ui| {
                ui.text(
                    track.title,
                    Modifier::new()
                        .font_size(19.sp())
                        .font_weight(FontWeight::SEMI_BOLD)
                        .max_lines(1),
                );
                ui.text(
                    track.artist,
                    Modifier::new().font_size(15.sp()).color(palette.muted),
                );
            });
            ui.space(Modifier::new());
            ui.text(
                track.length,
                Modifier::new().font_size(16.sp()).color(palette.muted),
            );
        },
    );
}

fn taskbar(ui: &mut Ui, palette: &Palette, icons: SceneIcons, size: Size) {
    let glass = Material::new(palette.taskbar)
        .backdrop(constant!(Backdrop::blur(Dp::new(3.0))))
        .border(Stroke {
            color: palette.taskbar_edge,
            ..EDGE
        });
    let width = size.width - MARGIN * 2.0;
    let travel = match NonNegativeDp::saturating(PANEL_HEIGHT.get() - (TASKBAR + MARGIN)) {
        Ok(travel) => travel,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let returning = IslandReturn::new(ui, travel);
    let fade = Duration::from_millis(120);
    let showing = Tween::new(fade).delay(returning.content_after);
    ui.shared(
        TASKBAR_GLASS,
        Modifier::new()
            .offset(MARGIN, size.height - MARGIN - TASKBAR)
            .size(width, TASKBAR)
            .material(glass, TASKBAR / 2.0)
            .transition(Transition::new(returning.glass).reflow()),
        |ui| {
            ui.key(
                "content",
                Modifier::new()
                    .transition(Transition::new(Tween::new(fade)).exit(Presence::fade())),
                |ui| {
                    ui.row(
                        Modifier::new()
                            .gap(20.dp())
                            .align_items(Align::Center)
                            .size(width, TASKBAR)
                            .padding_xy(12.dp(), Dp::ZERO)
                            .transition(Transition::new(showing).enter(Presence::fade())),
                        |ui| taskbar_content(ui, palette, icons),
                    );
                },
            );
        },
    );
}

fn taskbar_content(ui: &mut Ui, palette: &Palette, icons: SceneIcons) {
    let (ink, on_app) = (palette.ink, palette.on_app);
    let ring = Some((palette.launcher_ring, 2.dp()));
    disc(ui, palette.launcher, ring, |ui| {
        ui.icon(
            icons.launcher,
            IconOptions {
                size: Some(DISC_ICON),
                ..IconOptions::default()
            },
            Modifier::new().color(ink),
        );
    });
    disc(ui, palette.app, Some((palette.accent, 3.dp())), |ui| {
        ui.icon(
            icons.music,
            IconOptions {
                size: Some(DISC_ICON),
                ..IconOptions::default()
            },
            Modifier::new().color(on_app),
        );
    });
    disc(ui, palette.navigation, None, |ui| {
        ui.icon(
            icons.navigation,
            IconOptions {
                size: Some(DISC_ICON),
                ..IconOptions::default()
            },
            Modifier::new().color(on_app),
        );
    });
    ui.space(Modifier::new());
    ui.row(
        Modifier::new().gap(8.dp()).align_items(Align::Center),
        |ui| {
            ui.icon(
                icons.signal,
                IconOptions {
                    size: Some(18.dp()),
                    ..IconOptions::default()
                },
                Modifier::new().color(ink),
            );
            ui.icon(
                icons.battery,
                IconOptions {
                    size: Some(22.dp()),
                    ..IconOptions::default()
                },
                Modifier::new().color(ink),
            );
            ui.text(
                "14:32",
                Modifier::new()
                    .font_size(20.sp())
                    .font_weight(FontWeight::SEMI_BOLD)
                    .padding_xy(6.dp(), Dp::ZERO),
            );
        },
    );
}

fn disc(ui: &mut Ui, fill: Color, ring: Option<(Color, Dp)>, icon: impl FnOnce(&mut Ui)) {
    let corners = DISC / 2.0;
    let mut modifier = Modifier::new().material(Material::new(fill), corners);
    if let Some((color, width)) = ring {
        modifier = modifier.outline(color, width, corners, 3.dp());
    }
    ui.stack(modifier.padding((DISC - DISC_ICON) / 2.0), icon);
}
