// SPDX-License-Identifier: GPL-3.0-only
use super::look::{
    self, BUTTON_HEIGHT, BUTTON_PADDING, BUTTON_RADIUS, ICON_SIZE, ISLAND_PADDING, ISLAND_RADIUS,
    MUTED, SPACING,
};
use super::model::{App, Playlist, Taskbar, Track};
use crate::{CopyKey, copy};
use lavashell::prelude::*;
use lavashell_example_support::cover;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Island {
    Apps,
    Media,
    Status,
}

impl Island {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Apps => "apps",
            Self::Media => "media",
            Self::Status => "status",
        }
    }
}

pub const BAR: &str = "bar";
pub const TRACK: &str = "track";

pub fn bar(ui: &mut Ui, taskbar: &Taskbar) {
    ui.row(
        Modifier::new()
            .tag(BAR)
            .align_items(Align::End)
            .fill_max_width()
            .material_group(look::glass(taskbar.daylight.get()), SPACING)
            .transition(Transition::new(look::spring()).reflow()),
        |ui| {
            ui.stack(island(Island::Apps), |ui| apps(ui, taskbar));
            ui.space(Modifier::new().weight(if taskbar.docked.get() { 0.0 } else { 1.0 }));
            ui.stack(island(Island::Media), |ui| media(ui, taskbar));
            ui.space(Modifier::new());
            status(ui, taskbar);
        },
    );
}

fn island(island: Island) -> Modifier {
    Modifier::new()
        .tag(island.tag())
        .island(ISLAND_RADIUS)
        .clip(ISLAND_RADIUS)
        .padding(ISLAND_PADDING)
        .transition(Transition::new(look::spring()).reflow())
}

fn apps(ui: &mut Ui, taskbar: &Taskbar) {
    ui.row(Modifier::new().gap(4.dp()), |ui| {
        for app in App::ALL {
            slot(ui, taskbar, app);
        }
    });
}

fn slot(ui: &mut Ui, taskbar: &Taskbar, app: App) {
    let styles = ui.state(app, || look::slot(app.accent())).get();
    let selected = taskbar.foreground.get() == app;
    ui.button(
        Modifier::new()
            .selected(selected)
            .state_styles(styles)
            .tag(app.name())
            .accessible_name(app.name())
            .clip(BUTTON_RADIUS)
            .padding(BUTTON_PADDING)
            .transition(Transition::new(look::spring()).reflow()),
        taskbar
            .foreground
            .action(move |foreground| *foreground = app),
        |ui| {
            ui.row(
                Modifier::new().gap(10.dp()).align_items(Align::Center),
                |ui| {
                    ui.icon(
                        taskbar.icons.of(app),
                        IconOptions {
                            size: Some(ICON_SIZE),
                            ..IconOptions::default()
                        },
                        Modifier::new(),
                    );
                    if selected {
                        ui.text(
                            app.name(),
                            Modifier::new()
                                .font_weight(FontWeight::SEMI_BOLD)
                                .transition(appearing(fade_slide(FROM_LEFT))),
                        );
                    }
                },
            );
        },
    );
}

fn media(ui: &mut Ui, taskbar: &Taskbar) {
    let styles = ui.state(Island::Media, look::media).get();
    let track = taskbar.playlist.get().current();
    ui.column(
        Modifier::new().gap(14.dp()).align_items(Align::Center),
        |ui| {
            if taskbar.expanded.get() {
                ui.key(
                    "card",
                    Modifier::new().transition(appearing(
                        fade_slide(FROM_BELOW).scale(constant!(ScaleFactor::try_new(0.9))),
                    )),
                    |ui| card(ui, track),
                );
            }
            ui.key(
                "controls",
                Modifier::new().transition(look::spring()),
                |ui| controls(ui, taskbar, track, &styles),
            );
        },
    );
}

fn card(ui: &mut Ui, track: Track) {
    ui.row(
        Modifier::new()
            .gap(18.dp())
            .align_items(Align::Center)
            .padding(8.dp()),
        |ui| {
            ui.canvas(Size::new(120.dp(), 120.dp()), Modifier::new(), move |d| {
                cover(d, track.hue);
            });
            ui.column(Modifier::new().gap(6.dp()), |ui| {
                ui.text(track.title, Modifier::new().text_style(look::title()));
                ui.text(track.artist, Modifier::new().color(MUTED));
            });
        },
    );
}

fn controls(ui: &mut Ui, taskbar: &Taskbar, track: Track, styles: &StateStyles) {
    ui.row(
        Modifier::new().gap(4.dp()).align_items(Align::Center),
        |ui| {
            let inset = 4.dp();
            let side = BUTTON_HEIGHT - inset * 2.0;
            ui.button(
                Modifier::new()
                    .selected(taskbar.expanded.get())
                    .state_styles(styles.clone())
                    .tag(TRACK)
                    .clip(BUTTON_RADIUS)
                    .insets(Insets {
                        left: inset,
                        top: inset,
                        right: 16.dp(),
                        bottom: inset,
                    }),
                taskbar.expanded.action(|open| *open = !*open),
                |ui| {
                    ui.row(
                        Modifier::new().gap(10.dp()).align_items(Align::Center),
                        |ui| {
                            ui.canvas(Size::new(side, side), Modifier::new(), move |d| {
                                cover(d, track.hue);
                            });
                            ui.column(Modifier::new(), |ui| {
                                ui.text(
                                    track.title,
                                    Modifier::new().font_weight(FontWeight::SEMI_BOLD),
                                );
                                ui.text(
                                    track.artist,
                                    Modifier::new().color(MUTED).font_size(12.sp()),
                                );
                            });
                        },
                    );
                },
            );
            media_button(
                ui,
                taskbar.icons.skip_back,
                copy(CopyKey::Previous),
                styles,
                Modifier::new(),
                taskbar.playlist.action(Playlist::previous),
            );
            let playing = taskbar.playing.get();
            media_button(
                ui,
                if playing {
                    taskbar.icons.pause
                } else {
                    taskbar.icons.play
                },
                copy(if playing {
                    CopyKey::TrackPause
                } else {
                    CopyKey::TrackPlay
                }),
                styles,
                Modifier::new().selected(playing),
                taskbar.playing.action(|playing| *playing = !*playing),
            );
            media_button(
                ui,
                taskbar.icons.skip_forward,
                copy(CopyKey::Next),
                styles,
                Modifier::new(),
                taskbar.playlist.action(Playlist::next),
            );
        },
    );
}

fn media_button(
    ui: &mut Ui,
    icon: &'static Icon,
    name: &str,
    styles: &StateStyles,
    modifier: Modifier,
    action: impl FnMut() + 'static,
) {
    let own = Modifier::new()
        .accessible_name(name)
        .tag(name)
        .state_styles(styles.clone())
        .padding(BUTTON_PADDING);
    let options = IconOptions {
        size: Some(ICON_SIZE),
        ..IconOptions::default()
    };
    ui.icon_button(icon, options, own.then(modifier), action);
}

fn status(ui: &mut Ui, taskbar: &Taskbar) {
    let styles = ui.state(Island::Status, look::status).get();
    let modifier = island(Island::Status).state_styles(styles);
    ui.button(
        modifier,
        taskbar.trip.action(|shown| *shown = !*shown),
        |ui| {
            ui.row(
                Modifier::new()
                    .gap(16.dp())
                    .align_items(Align::Center)
                    .height(BUTTON_HEIGHT)
                    .padding_xy(14.dp(), 0.dp()),
                |ui| {
                    ui.text("21°", Modifier::new());
                    let weight = constant!(FontWeight::try_new(650.0));
                    ui.text("12:45", Modifier::new().font_weight(weight));
                    if taskbar.trip.get() {
                        ui.row(
                            Modifier::new()
                                .gap(8.dp())
                                .align_items(Align::Center)
                                .transition(appearing(fade_slide(FROM_RIGHT))),
                            |ui| {
                                ui.icon(
                                    taskbar.icons.navigation,
                                    IconOptions {
                                        size: Some(18.dp()),
                                        ..IconOptions::default()
                                    },
                                    Modifier::new().color(App::Navigation.accent()),
                                );
                                ui.text(
                                    "13:07",
                                    Modifier::new().font_weight(FontWeight::SEMI_BOLD),
                                );
                                ui.text("24 km", Modifier::new().color(MUTED));
                            },
                        );
                    }
                },
            );
        },
    );
}

fn appearing(presence: Presence) -> Transition {
    Transition::new(look::spring())
        .enter(presence)
        .exit(Presence::fade())
}

const fn offset(x: f64, y: f64) -> Result<FinitePoint, NumberError> {
    FinitePoint::try_new(Point::new(Dp::new(x), Dp::new(y)))
}

const FROM_LEFT: FinitePoint = constant!(offset(-10.0, 0.0));
const FROM_RIGHT: FinitePoint = constant!(offset(16.0, 0.0));
const FROM_BELOW: FinitePoint = constant!(offset(0.0, 12.0));

fn fade_slide(offset: FinitePoint) -> Presence {
    Presence::fade().slide(offset)
}
