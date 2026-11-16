// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CopyKey, Model,
    art::album_art,
    catalog::TrackId,
    controls::{heading, line, text},
    copy,
    player::duration,
    screen::Screen,
    tag::Tag,
};
use lavashell::prelude::*;
use lavashell_example_support::{Layout, TEAL_PALETTE};

fn track_header(ui: &mut Ui, layout: Layout) {
    let colors = ui.local(&TEAL_PALETTE);
    ui.row(
        Modifier::new()
            .gap(14.dp())
            .fill_max_width()
            .padding_xy(12.dp(), 6.dp()),
        |ui| {
            ui.spacer(24.dp(), 1.dp(), Modifier::new());
            ui.text(
                copy(CopyKey::TrackHeader),
                Modifier::new()
                    .font_size(10.sp())
                    .color(colors.muted)
                    .weight(2.0),
            );
            ui.text(
                copy(CopyKey::ArtistHeader),
                Modifier::new()
                    .font_size(10.sp())
                    .color(colors.muted)
                    .weight(1.0),
            );
            if layout == Layout::Regular {
                ui.text(
                    copy(CopyKey::AlbumHeader),
                    Modifier::new()
                        .font_size(10.sp())
                        .color(colors.muted)
                        .weight(1.0),
                );
            }
            ui.text(
                copy(CopyKey::DurationHeader),
                Modifier::new()
                    .font_size(10.sp())
                    .color(colors.muted)
                    .width(64.dp()),
            );
            ui.spacer(24.dp(), 1.dp(), Modifier::new());
        },
    );
}

fn empty_hint(ui: &mut Ui, m: &Model) {
    let colors = ui.local(&TEAL_PALETTE);
    if m.query.is_empty() {
        text(ui, copy(CopyKey::CollectionEmpty), 15.sp(), colors.text);
        text(
            ui,
            copy(CopyKey::CollectionEmptyHint),
            13.sp(),
            colors.muted,
        );
    } else {
        text(ui, copy(CopyKey::SearchEmpty), 15.sp(), colors.text);
        text(ui, copy(CopyKey::SearchHint), 13.sp(), colors.muted);
    }
}

pub(super) fn track_list(ui: &mut Ui, model: &State<Model>, screen: Screen) {
    let colors = ui.local(&TEAL_PALETTE);
    let Screen { icons, layout } = screen;
    let m = model.get();
    let ids = m.visible();
    heading(ui, copy(CopyKey::Tracks), 19.sp());
    track_header(ui, layout);
    line(ui);

    if ids.is_empty() {
        empty_hint(ui, &m);
    }

    for id in ids {
        let track = id.track();
        let album = track.album.album();
        let current = m.current == id;
        ui.key(id, Modifier::new(), |ui| {
            ui.row(
                Modifier::new()
                    .gap(14.dp())
                    .align_items(Align::Center)
                    .tag(Tag::Track(id))
                    .accessible_name(track.title)
                    .selected(current)
                    .on_click(model.action(move |m| m.select(id)))
                    .fill_max_width()
                    .clip(8.dp())
                    .background(if current {
                        colors.active
                    } else {
                        colors.background
                    })
                    .padding_xy(12.dp(), 7.dp()),
                |ui| {
                    if current {
                        ui.icon(
                            icons.music,
                            IconOptions {
                                size: Some(14.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(colors.accent).width(24.dp()),
                        );
                    } else {
                        ui.text(
                            format!("{:02}", id.number()),
                            Modifier::new()
                                .font_size(11.sp())
                                .color(colors.muted)
                                .width(24.dp()),
                        );
                    }
                    ui.row(
                        Modifier::new()
                            .gap(12.dp())
                            .align_items(Align::Center)
                            .weight(2.0),
                        |ui| {
                            album_art(ui, track.album, Size::new(36.dp(), 36.dp()));
                            ui.text(
                                track.title,
                                Modifier::new()
                                    .font_size(13.sp())
                                    .color(if current { colors.accent } else { colors.text })
                                    .weight(1.0),
                            );
                        },
                    );
                    ui.text(
                        album.artist,
                        Modifier::new()
                            .font_size(12.sp())
                            .color(colors.muted)
                            .weight(1.0),
                    );
                    if layout == Layout::Regular {
                        ui.text(
                            album.title,
                            Modifier::new()
                                .font_size(12.sp())
                                .color(colors.muted)
                                .weight(1.0),
                        );
                    }
                    ui.text(
                        duration(track.duration),
                        Modifier::new()
                            .font_size(11.sp())
                            .color(colors.muted)
                            .width(64.dp()),
                    );
                    favorite_button(ui, model, icons.heart, id, m.favorites.contains(&id));
                },
            );
        });
    }
}

fn favorite_button(
    ui: &mut Ui,
    model: &State<Model>,
    heart: &'static Icon,
    id: TrackId,
    saved: bool,
) {
    let colors = ui.local(&TEAL_PALETTE);
    ui.icon_button(
        heart,
        IconOptions {
            size: Some(15.dp()),
            style: if saved {
                IconStyle::Filled
            } else {
                IconStyle::Outlined
            },
        },
        Modifier::new()
            .accessible_name(copy(if saved { CopyKey::Saved } else { CopyKey::Save }))
            .semantics_with(move |s| {
                s.state.checked = Some(if saved { Checked::True } else { Checked::False });
            })
            .color(if saved { colors.accent } else { colors.muted })
            .padding(4.dp()),
        model.action(move |m| m.toggle_favorite(id)),
    );
}
