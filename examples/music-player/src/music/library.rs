// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CopyKey, Icons, Model, Section,
    art::album_art,
    catalog::{ALBUMS, AlbumId, FEATURED_ALBUM, PLAYLISTS, TrackId},
    controls::{heading, line, nav, text},
    copy,
    player::player,
    screen::Screen,
    tag::Tag,
    tracks::track_list,
};
use crate::theme::{scope, title};
use lavashell::prelude::*;
use lavashell_example_support::{Breakpoint, Layout, TEAL_PALETTE, Theme, ThemeSwitchColors};

pub fn music(ui: &mut Ui, model: &State<Model>, theme: &Theme, icons: Icons) {
    scope(ui, theme, |ui| music_content(ui, model, theme, icons));
}

fn music_content(ui: &mut Ui, model: &State<Model>, theme: &Theme, icons: Icons) {
    let colors = ui.local(&TEAL_PALETTE);
    let widths = Breakpoint::new(ui, "widths");
    let layout = widths.layout(|full, given| full.minimum.width <= given.width);
    let screen = Screen { icons, layout };
    let heights = Breakpoint::new(ui, "heights");
    let hero_layout = heights.layout(|hero, given| hero.preferred.height <= given.height);
    ui.column(
        Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .background(colors.background)
            .then(layout.container(&widths)),
        |ui| {
            ui.row(Modifier::new().weight(1.0).fill_max_width(), |ui| {
                sidebar(ui, model, screen);
                ui.column(Modifier::new().weight(1.0).fill_max_height(), |ui| {
                    header(ui, model, theme, icons);
                    ui.scroll(
                        Axis::Vertical,
                        Modifier::new()
                            .gap(10.dp())
                            .tag(Tag::CollectionScroll)
                            .weight(1.0)
                            .fill_max_width()
                            .padding_xy(layout.choose(24.dp(), 32.dp()), 24.dp())
                            .then(heights.given_size()),
                        |ui| {
                            let m = model.get();
                            let section_title = match m.section {
                                Section::Collection => copy(CopyKey::CollectionTitle),
                                Section::Favorites => copy(CopyKey::NavFavorites),
                                Section::Recent => copy(CopyKey::NavRecent),
                                Section::Playlist(playlist) => playlist.name,
                            };
                            ui.text(
                                section_title,
                                Modifier::new()
                                    .text_style(title(colors.text, 32.sp()))
                                    .semantics_with(|s| s.role = Some(Role::Heading)),
                            );
                            if m.section == Section::Collection && m.query.is_empty() {
                                if hero_layout == Layout::Regular {
                                    ui.spacer(0.dp(), 8.dp(), Modifier::new());
                                    hero(ui, model, screen, &heights);
                                    ui.spacer(0.dp(), 6.dp(), Modifier::new());
                                }
                                heading(ui, copy(CopyKey::Albums), 19.sp());
                                albums(ui, model);
                                ui.spacer(0.dp(), 6.dp(), Modifier::new());
                            }
                            track_list(ui, model, screen);
                        },
                    );
                });
            });
            player(ui, model, screen);
        },
    );
}

fn brand(ui: &mut Ui) {
    let colors = ui.local(&TEAL_PALETTE);
    ui.row(
        Modifier::new()
            .gap(10.dp())
            .align_items(Align::Center)
            .padding_xy(10.dp(), 8.dp()),
        |ui| {
            ui.canvas(Size::new(24.dp(), 24.dp()), Modifier::new(), move |d| {
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
                        colors.accent,
                    );
                }
            });
            ui.text(
                copy(CopyKey::Brand),
                Modifier::new().text_style(title(colors.text, 22.sp())),
            );
        },
    );
}

fn sidebar(ui: &mut Ui, model: &State<Model>, screen: Screen) {
    let colors = ui.local(&TEAL_PALETTE);
    let Screen { icons, layout } = screen;
    ui.column(
        Modifier::new()
            .gap(8.dp())
            .width(layout.choose(190.dp(), 218.dp()))
            .fill_max_height()
            .background(colors.sidebar)
            .padding(20.dp()),
        |ui| {
            brand(ui);

            ui.spacer(0.dp(), 24.dp(), Modifier::new());
            text(
                ui,
                copy(CopyKey::SectionLibrary).to_uppercase(),
                10.sp(),
                colors.muted,
            );
            for (section, icon, key) in [
                (Section::Collection, icons.music, CopyKey::NavCollection),
                (Section::Favorites, icons.heart, CopyKey::NavFavorites),
                (Section::Recent, icons.clock, CopyKey::NavRecent),
            ] {
                nav(ui, model, section, icon, key);
            }

            ui.spacer(0.dp(), 18.dp(), Modifier::new());
            line(ui);
            ui.spacer(0.dp(), 14.dp(), Modifier::new());
            text(
                ui,
                copy(CopyKey::SectionPlaylists).to_uppercase(),
                10.sp(),
                colors.muted,
            );
            for playlist in PLAYLISTS {
                ui.row(
                    Modifier::new()
                        .gap(12.dp())
                        .align_items(Align::Center)
                        .on_activate(model.handler(move |m, _| {
                            m.section = Section::Playlist(playlist);
                            m.album = None;
                        }))
                        .accessible_name(playlist.name)
                        .selected(model.get().section == Section::Playlist(playlist))
                        .fill_max_width()
                        .padding_xy(12.dp(), 9.dp()),
                    |ui| {
                        ui.icon(
                            icons.list,
                            IconOptions {
                                size: Some(17.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(colors.muted),
                        );
                        text(ui, playlist.name, 13.sp(), colors.muted);
                    },
                );
            }

            ui.space(Modifier::new());
            let m = model.get();
            let id = m.current.track().album;
            let album = id.album();
            album_art(ui, id, Size::new(128.dp(), 128.dp()));
            heading(ui, album.title, 13.sp());
            text(ui, album.artist, 12.sp(), colors.muted);
        },
    );
}

fn header(ui: &mut Ui, model: &State<Model>, theme: &Theme, icons: Icons) {
    let colors = ui.local(&TEAL_PALETTE);
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
                    .background(colors.card)
                    .padding_xy(14.dp(), 10.dp()),
                |ui| {
                    ui.icon(
                        icons.search,
                        IconOptions {
                            size: Some(18.dp()),
                            ..IconOptions::default()
                        },
                        Modifier::new().color(colors.muted),
                    );
                    ui.text_field(
                        model.get().query,
                        TextFieldOptions {
                            placeholder: copy(CopyKey::Search).into(),
                            ..TextFieldOptions::default()
                        },
                        Modifier::new()
                            .font_size(13.sp())
                            .color(colors.text)
                            .weight(1.0)
                            .tag(Tag::SearchInput)
                            .accessible_name(copy(CopyKey::Search)),
                        model.handler(|m, value| m.query = value),
                    );
                },
            );
            ui.space(Modifier::new());
            theme.switch(
                ui,
                ThemeSwitchColors {
                    ink: colors.muted,
                    accent: colors.accent,
                },
            );
            ui.text(
                copy(CopyKey::LocalBadge),
                Modifier::new()
                    .font_size(11.sp())
                    .color(colors.accent)
                    .clip(5.dp())
                    .background(colors.active)
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

fn hero(ui: &mut Ui, model: &State<Model>, screen: Screen, heights: &Breakpoint) {
    let colors = ui.local(&TEAL_PALETTE);
    let Screen { icons, layout } = screen;
    let id = model.get().album.unwrap_or(FEATURED_ALBUM);
    let album = id.album();
    let art = layout.choose(148.dp(), 172.dp());
    ui.row(
        Modifier::new()
            .gap(layout.choose(22.dp(), 32.dp()))
            .align_items(Align::Center)
            .fill_max_width()
            .clip(12.dp())
            .background(Gradient::new(
                GradientKind::Linear {
                    from: Point::default(),
                    to: Point::new(800.dp(), 160.dp()),
                },
                [
                    ColorStop::new(0.0, colors.active),
                    ColorStop::new(1.0, colors.card),
                ],
            ))
            .padding(20.dp())
            .tag(Tag::Hero)
            .then(heights.content_sizes()),
        |ui| {
            album_art(ui, id, Size::new(art, art));
            ui.column(Modifier::new().gap(9.dp()).weight(1.0), |ui| {
                ui.spacer(0.dp(), 8.dp(), Modifier::new());
                text(
                    ui,
                    copy(CopyKey::Featured).to_uppercase(),
                    10.sp(),
                    colors.accent,
                );
                heading(ui, album.title, layout.choose(32.sp(), 38.sp()));
                text(
                    ui,
                    format!("{}  ·  {}  ·  {}", album.artist, album.genre, album.year),
                    13.sp(),
                    colors.muted,
                );
                ui.spacer(0.dp(), 10.dp(), Modifier::new());
                let Some(first_track) =
                    TrackId::all().find(|candidate| candidate.track().album == id)
                else {
                    return;
                };
                ui.row(
                    Modifier::new()
                        .gap(8.dp())
                        .align_items(Align::Center)
                        .tag(Tag::PlayAlbum)
                        .on_activate(model.handler(move |m, _| {
                            m.album = Some(id);
                            m.select(first_track);
                        }))
                        .clip(22.dp())
                        .background(colors.accent)
                        .padding_xy(20.dp(), 11.dp()),
                    |ui| {
                        ui.icon(
                            icons.play,
                            IconOptions {
                                size: Some(17.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(colors.on_accent),
                        );
                        ui.text(
                            copy(CopyKey::PlayAlbum),
                            Modifier::new()
                                .font_size(13.sp())
                                .font_weight(FontWeight::BOLD)
                                .color(colors.on_accent),
                        );
                    },
                );
            });
        },
    );
}

const ALBUM_MIN_WIDTH: Dp = Dp::new(180.0);
const ALBUM_GAP: Dp = Dp::new(18.0);

fn albums(ui: &mut Ui, model: &State<Model>) {
    let width = ui.state("album-row-width", || Dp::ZERO);
    let measured = width.clone();
    ui.row(
        Modifier::new()
            .gap(ALBUM_GAP)
            .fill_max_width()
            .on_measure(move |size| measured.set_if_changed(size.width)),
        |ui| {
            for id in AlbumId::all().take(albums_that_fit(width.get())) {
                album_card(ui, model, id);
            }
        },
    );
}

fn albums_that_fit(width: Dp) -> usize {
    std::iter::successors(Some(ALBUM_MIN_WIDTH), |needed| {
        Some(*needed + ALBUM_GAP + ALBUM_MIN_WIDTH)
    })
    .take(ALBUMS.len())
    .take_while(|needed| *needed <= width)
    .count()
    .max(1)
}

fn album_card(ui: &mut Ui, model: &State<Model>, id: AlbumId) {
    let colors = ui.local(&TEAL_PALETTE);
    let album = id.album();
    ui.key(id, Modifier::new().weight(1.0), |ui| {
        ui.column(
            Modifier::new()
                .gap(6.dp())
                .on_activate(model.handler(move |m, _| m.album = Some(id)))
                .accessible_name(album.title)
                .selected(model.get().album == Some(id))
                .fill_max_width(),
            |ui| {
                album_art(ui, id, Size::new(0.dp(), 112.dp()));
                heading(ui, album.title, 14.sp());
                text(ui, album.artist, 12.sp(), colors.muted);
            },
        );
    });
}
