// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CopyKey, Icons, Model, Section,
    art::album_art,
    catalog::{ALBUMS, PLAYLISTS, TRACKS},
    controls::{heading, line, nav, text},
    copy,
    player::player,
    tracks::track_list,
};
use crate::theme::{ACCENT, BG, CARD, MUTED, SIDE, TEXT, scope, title};
use lavashell::prelude::*;
use lavashell_example_support::Breakpoint;

/// A design decision of the demo: a window shorter than this leaves out the featured album, so that
/// the albums and the tracks start higher.
const FEATURED_MIN_WINDOW_HEIGHT: Dp = Dp::new(900.0);

pub fn music(ui: &mut Ui, model: &State<Model>, icons: Icons) {
    scope(ui, |ui| music_content(ui, model, icons));
}

fn music_content(ui: &mut Ui, model: &State<Model>, icons: Icons) {
    let widths = Breakpoint::new(ui, "widths");
    let compact = !widths.fits(|full, given| full.minimum.width <= given.width);
    let short = ui.viewport().size.height < FEATURED_MIN_WINDOW_HEIGHT;
    ui.column(
        Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .background(BG)
            .then(widths.container(!compact)),
        |ui| {
            ui.row(Modifier::new().weight(1.0).fill_max_width(), |ui| {
                sidebar(ui, model, icons, compact);
                ui.column(Modifier::new().weight(1.0).fill_max_height(), |ui| {
                    header(ui, model, icons);
                    ui.scroll(
                        Axis::Vertical,
                        Modifier::new()
                            .gap(10.dp())
                            .tag("collection-scroll")
                            .weight(1.0)
                            .fill_max_width()
                            .padding_xy(if compact { 24.dp() } else { 32.dp() }, 24.dp()),
                        |ui| {
                            let m = model.get();
                            let section_title = match m.section {
                                Section::Collection => copy(CopyKey::CollectionTitle),
                                Section::Favorites => copy(CopyKey::NavFavorites),
                                Section::Recent => copy(CopyKey::NavRecent),
                                Section::Playlist(p) => PLAYLISTS[p],
                            };
                            ui.text(
                                section_title,
                                Modifier::new()
                                    .text_style(title(32.sp()))
                                    .semantics_with(|s| s.role = Some(Role::Heading)),
                            );
                            if m.section == Section::Collection && m.query.is_empty() {
                                text(ui, copy(CopyKey::CollectionDescription), 13.sp(), MUTED);
                                if !short {
                                    ui.spacer(0.dp(), 8.dp(), Modifier::new());
                                    hero(ui, model, icons, compact);
                                    ui.spacer(0.dp(), 6.dp(), Modifier::new());
                                }
                                heading(ui, copy(CopyKey::Albums), 19.sp());
                                albums(ui, model);
                                ui.spacer(0.dp(), 6.dp(), Modifier::new());
                            }
                            track_list(ui, model, icons, compact);
                        },
                    );
                });
            });
            player(ui, model, icons, compact);
        },
    );
}

fn brand(ui: &mut Ui) {
    ui.row(
        Modifier::new()
            .gap(10.dp())
            .align_items(Align::Center)
            .padding_xy(10.dp(), 8.dp()),
        |ui| {
            ui.canvas(Size::new(24.dp(), 24.dp()), Modifier::new(), |d| {
                for bar in 0..4_u8 {
                    d.fill(
                        Shape::rounded(
                            Rect::new(
                                Dp::new(f64::from(bar * 6)),
                                if bar % 2 == 0 { 2.dp() } else { 7.dp() },
                                3.dp(),
                                if bar % 2 == 0 { 20.dp() } else { 10.dp() },
                            ),
                            1.5.dp(),
                        ),
                        ACCENT,
                    );
                }
            });
            ui.text(
                copy(CopyKey::Brand),
                Modifier::new().text_style(title(22.sp())),
            );
        },
    );
}

fn sidebar(ui: &mut Ui, model: &State<Model>, icons: Icons, compact: bool) {
    ui.column(
        Modifier::new()
            .gap(8.dp())
            .width(if compact { 190.dp() } else { 218.dp() })
            .fill_max_height()
            .background(SIDE)
            .padding(20.dp()),
        |ui| {
            brand(ui);

            ui.spacer(0.dp(), 24.dp(), Modifier::new());
            text(
                ui,
                copy(CopyKey::SectionLibrary).to_uppercase(),
                10.sp(),
                MUTED,
            );
            for (section, icon, key, tag) in [
                (
                    Section::Collection,
                    icons.music,
                    CopyKey::NavCollection,
                    "nav-collection",
                ),
                (
                    Section::Favorites,
                    icons.heart,
                    CopyKey::NavFavorites,
                    "nav-favorites",
                ),
                (
                    Section::Recent,
                    icons.clock,
                    CopyKey::NavRecent,
                    "nav-recent",
                ),
            ] {
                nav(ui, model, section, icon, key, tag);
            }

            ui.spacer(0.dp(), 18.dp(), Modifier::new());
            line(ui);
            ui.spacer(0.dp(), 14.dp(), Modifier::new());
            text(
                ui,
                copy(CopyKey::SectionPlaylists).to_uppercase(),
                10.sp(),
                MUTED,
            );
            for (index, name) in PLAYLISTS.into_iter().enumerate() {
                ui.row(
                    Modifier::new()
                        .gap(12.dp())
                        .align_items(Align::Center)
                        .on_click(model.action(move |m| {
                            m.section = Section::Playlist(index);
                            m.album = None;
                        }))
                        .accessible_name(name)
                        .selected(model.get().section == Section::Playlist(index))
                        .fill_max_width()
                        .padding_xy(12.dp(), 9.dp()),
                    |ui| {
                        ui.icon(
                            icons.list,
                            IconOptions {
                                size: Some(17.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(MUTED),
                        );
                        text(ui, name, 13.sp(), MUTED);
                    },
                );
            }

            ui.space(Modifier::new());
            let m = model.get();
            let album = TRACKS[m.current].album;
            album_art(ui, album, Size::new(128.dp(), 128.dp()));
            heading(ui, ALBUMS[album].title, 13.sp());
            text(ui, ALBUMS[album].artist, 12.sp(), MUTED);
        },
    );
}

fn header(ui: &mut Ui, model: &State<Model>, icons: Icons) {
    ui.row(
        Modifier::new()
            .gap(18.dp())
            .align_items(Align::Center)
            .fill_max_width()
            .padding_xy(32.dp(), 20.dp()),
        |ui| {
            ui.row(
                Modifier::new()
                    .gap(12.dp())
                    .align_items(Align::Center)
                    .width(340.dp())
                    .clip(8.dp())
                    .background(CARD)
                    .padding_xy(14.dp(), 10.dp()),
                |ui| {
                    ui.icon(
                        icons.search,
                        IconOptions {
                            size: Some(18.dp()),
                            ..IconOptions::default()
                        },
                        Modifier::new().color(MUTED),
                    );
                    ui.text_field(
                        model.get().query,
                        TextFieldOptions {
                            placeholder: copy(CopyKey::Search).into(),
                            ..TextFieldOptions::default()
                        },
                        Modifier::new()
                            .font_size(13.sp())
                            .color(TEXT)
                            .weight(1.0)
                            .tag("search-input")
                            .accessible_name(copy(CopyKey::Search)),
                        model.handler(|m, value| m.query = value),
                    );
                },
            );
            ui.space(Modifier::new());
            ui.text(
                copy(CopyKey::LocalBadge),
                Modifier::new()
                    .font_size(11.sp())
                    .color(ACCENT)
                    .clip(5.dp())
                    .background(Color::rgb(0x0021_474d))
                    .padding_xy(10.dp(), 5.dp()),
            );
            ui.canvas(Size::new(32.dp(), 32.dp()), Modifier::new(), |d| {
                d.circle(
                    Point::new(16.dp(), 16.dp()),
                    16.dp(),
                    Color::rgb(0x00de_c49c),
                );
                d.circle(
                    Point::new(16.dp(), 12.dp()),
                    5.dp(),
                    Color::rgb(0x0066_5346),
                );
                d.circle(
                    Point::new(16.dp(), 29.dp()),
                    11.dp(),
                    Color::rgb(0x0066_5346),
                );
            });
        },
    );
    line(ui);
}

fn hero(ui: &mut Ui, model: &State<Model>, icons: Icons, compact: bool) {
    let index = model.get().album.unwrap_or(0);
    let album = &ALBUMS[index];
    ui.row(
        Modifier::new()
            .gap(if compact { 22.dp() } else { 32.dp() })
            .align_items(Align::Center)
            .fill_max_width()
            .clip(12.dp())
            .background(Gradient::new(
                GradientKind::Linear {
                    from: Point::default(),
                    to: Point::new(800.dp(), 160.dp()),
                },
                [
                    ColorStop::new(0.0, Color::rgb(0x001c_414c)),
                    ColorStop::new(1.0, Color::rgb(0x0016_343f)),
                ],
            ))
            .padding(20.dp()),
        |ui| {
            album_art(
                ui,
                index,
                Size::new(
                    if compact { 148.dp() } else { 172.dp() },
                    if compact { 148.dp() } else { 172.dp() },
                ),
            );
            ui.column(Modifier::new().gap(9.dp()).weight(1.0), |ui| {
                ui.spacer(0.dp(), 8.dp(), Modifier::new());
                text(ui, copy(CopyKey::Featured).to_uppercase(), 10.sp(), ACCENT);
                heading(ui, album.title, if compact { 32.sp() } else { 38.sp() });
                text(
                    ui,
                    format!("{}  ·  {}  ·  {}", album.artist, album.genre, album.year),
                    13.sp(),
                    MUTED,
                );
                ui.spacer(0.dp(), 10.dp(), Modifier::new());
                let Some(first_track) = TRACKS.iter().position(|t| t.album == index) else {
                    return;
                };
                ui.row(
                    Modifier::new()
                        .gap(8.dp())
                        .align_items(Align::Center)
                        .tag("play-album")
                        .on_click(model.action(move |m| {
                            m.album = Some(index);
                            m.select(first_track);
                        }))
                        .clip(22.dp())
                        .background(ACCENT)
                        .padding_xy(20.dp(), 11.dp()),
                    |ui| {
                        ui.icon(
                            icons.play,
                            IconOptions {
                                size: Some(17.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(SIDE),
                        );
                        ui.text(
                            copy(CopyKey::PlayAlbum),
                            Modifier::new()
                                .font_size(13.sp())
                                .font_weight(FontWeight::BOLD)
                                .color(SIDE),
                        );
                    },
                );
            });
        },
    );
}

/// A design decision of the demo: the smallest width of an album card. When the cards would be narrower, the row
/// shows one album fewer.
const ALBUM_MIN_WIDTH: Dp = Dp::new(180.0);
const ALBUM_GAP: Dp = Dp::new(18.0);

/// Composes a row with as many album cards as fit the measured width of the row.
fn albums(ui: &mut Ui, model: &State<Model>) {
    let width = ui.state("album-row-width", || Dp::ZERO);
    let measured = width.clone();
    ui.row(
        Modifier::new()
            .gap(ALBUM_GAP)
            .fill_max_width()
            .on_measure(move |size| measured.set_if_changed(size.width)),
        |ui| {
            for index in 0..albums_that_fit(width.get()) {
                album_card(ui, model, index);
            }
        },
    );
}

/// The number of album cards, one at least, that fit a row of `width` at their narrowest.
fn albums_that_fit(width: Dp) -> usize {
    std::iter::successors(Some(ALBUM_MIN_WIDTH), |needed| {
        Some(*needed + ALBUM_GAP + ALBUM_MIN_WIDTH)
    })
    .take(ALBUMS.len())
    .take_while(|needed| *needed <= width)
    .count()
    .max(1)
}

fn album_card(ui: &mut Ui, model: &State<Model>, index: usize) {
    ui.key(index, Modifier::new().weight(1.0), |ui| {
        ui.column(
            Modifier::new()
                .gap(6.dp())
                .on_click(model.action(move |m| m.album = Some(index)))
                .accessible_name(ALBUMS[index].title)
                .selected(model.get().album == Some(index))
                .fill_max_width(),
            |ui| {
                album_art(ui, index, Size::new(0.dp(), 112.dp()));
                heading(ui, ALBUMS[index].title, 14.sp());
                text(ui, ALBUMS[index].artist, 12.sp(), MUTED);
            },
        );
    });
}
