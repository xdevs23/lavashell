// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CopyKey, Icons, Model,
    art::album_art,
    catalog::{ALBUMS, TRACKS},
    controls::{Control, button, heading, text},
    copy,
};
use crate::theme::{ACCENT, LINE, MUTED, SIDE, scope};
use lavashell::prelude::*;
use std::time::Duration;

pub const PLAYER_HEIGHT: Dp = Dp::new(104.0);

pub(super) fn player(ui: &mut Ui, model: &State<Model>, icons: Icons, compact: bool) {
    let m = model.get();
    let track = &TRACKS[m.current];
    let toggle = if m.playing {
        CopyKey::Pause
    } else {
        CopyKey::Play
    };
    ui.row(
        Modifier::new()
            .gap(12.dp())
            .align_items(Align::Center)
            .fill_max_width()
            .height(PLAYER_HEIGHT)
            .background(SIDE)
            .padding_xy(24.dp(), 12.dp()),
        |ui| {
            now_playing(ui, model, icons.heart, compact);

            ui.column(
                Modifier::new()
                    .gap(7.dp())
                    .weight(1.0)
                    .padding_xy(20.dp(), 0.dp()),
                |ui| {
                    ui.row(
                        Modifier::new()
                            .gap(10.dp())
                            .align_items(Align::Center)
                            .fill_max_width(),
                        |ui| {
                            ui.space(Modifier::new());
                            button(ui, icons.shuffle, Control::Shuffle, m.shuffle, model, |m| {
                                m.shuffle = !m.shuffle;
                            });
                            button(ui, icons.skip_back, Control::Previous, false, model, |m| {
                                m.previous();
                            });
                            ui.icon_button(
                                if m.playing { icons.pause } else { icons.play },
                                IconOptions {
                                    size: Some(20.dp()),
                                    ..IconOptions::default()
                                },
                                Modifier::new()
                                    .color(SIDE)
                                    .tag("play-toggle")
                                    .accessible_name(copy(toggle))
                                    .tooltip(copy(toggle))
                                    .clip(22.dp())
                                    .background(ACCENT)
                                    .padding(10.dp()),
                                model.action(|m| m.playing = !m.playing),
                            );
                            button(ui, icons.skip_forward, Control::Next, false, model, |m| {
                                m.next();
                            });
                            button(ui, icons.repeat, Control::Repeat, m.repeat, model, |m| {
                                m.repeat = !m.repeat;
                            });
                            ui.space(Modifier::new());
                        },
                    );

                    ui.row(
                        Modifier::new()
                            .gap(10.dp())
                            .align_items(Align::Center)
                            .fill_max_width(),
                        |ui| {
                            text(ui, duration(m.position), 10.sp(), MUTED);
                            let played = model.clone();
                            slider(
                                ui,
                                move || played.with(Model::played),
                                0.dp(),
                                ACCENT,
                                true,
                                Modifier::new().weight(1.0).tag("seek"),
                                model.handler(|m, value| {
                                    let length = TRACKS[m.current].duration.as_secs_f64();
                                    if let Ok(position) =
                                        Duration::try_from_secs_f64(value * length)
                                    {
                                        m.position = position;
                                    }
                                }),
                            );
                            text(ui, duration(track.duration), 10.sp(), MUTED);
                        },
                    );
                },
            );

            if !compact {
                volume(ui, model, icons.volume);
            }
        },
    );
}

/// The art, the title and the artist of the current track, and the button that adds the track to the favorites.
fn now_playing(ui: &mut Ui, model: &State<Model>, heart: &'static Icon, compact: bool) {
    let m = model.get();
    let track = &TRACKS[m.current];
    let album = &ALBUMS[track.album];
    ui.row(
        Modifier::new()
            .gap(12.dp())
            .align_items(Align::Center)
            .width(if compact { 240.dp() } else { 290.dp() }),
        |ui| {
            album_art(ui, track.album, Size::new(52.dp(), 52.dp()));
            ui.column(Modifier::new().gap(5.dp()).weight(1.0), |ui| {
                heading(ui, track.title, 13.sp());
                text(ui, album.artist, 11.sp(), MUTED);
            });
            let current = m.current;
            button(
                ui,
                heart,
                Control::SaveCurrent,
                m.favorites.contains(&current),
                model,
                move |m| m.toggle_favorite(current),
            );
        },
    );
}

fn volume(ui: &mut Ui, model: &State<Model>, speaker: &'static Icon) {
    ui.row(
        Modifier::new()
            .gap(12.dp())
            .align_items(Align::Center)
            .width(160.dp()),
        |ui| {
            ui.icon(
                speaker,
                IconOptions {
                    size: Some(18.dp()),
                    ..IconOptions::default()
                },
                Modifier::new().color(MUTED),
            );
            let volume = model.clone();
            slider(
                ui,
                move || volume.with(|m| m.volume),
                96.dp(),
                MUTED,
                false,
                Modifier::new()
                    .tag("volume")
                    .accessible_name(copy(CopyKey::Volume))
                    .tooltip(copy(CopyKey::Volume)),
                model.handler(|m, value| m.volume = value),
            );
        },
    );
}

pub fn panel(ui: &mut Ui, model: &State<Model>, icons: Icons) {
    scope(ui, |ui| player(ui, model, icons, false));
}

pub(super) fn duration(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn slider(
    ui: &mut Ui,
    value: impl Fn() -> f64 + 'static,
    width: Dp,
    color: Color,
    thumb: bool,
    modifier: Modifier,
    change: impl FnMut(f64) + 'static,
) {
    let own = if width == Dp::ZERO {
        Modifier::new().height(14.dp())
    } else {
        Modifier::new().height(14.dp()).width(width)
    };
    ui.slider(value, None, own.then(modifier), change, |ui, value| {
        ui.canvas(
            Size::new(width, 14.dp()),
            if width == Dp::ZERO {
                Modifier::new().fill_max_width()
            } else {
                Modifier::new().width(width)
            },
            move |d| {
                let value = value();
                let width = d.size().width;
                d.fill(
                    Shape::rounded(Rect::new(0.dp(), 5.dp(), width, 3.dp()), 1.5.dp()),
                    LINE,
                );
                d.fill(
                    Shape::rounded(Rect::new(0.dp(), 5.dp(), width * value, 3.dp()), 1.5.dp()),
                    color,
                );
                if thumb {
                    d.circle(Point::new(width * value, 6.5.dp()), 4.dp(), color);
                }
            },
        );
    });
}
