// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CopyKey, Icons, Model,
    art::album_art,
    catalog::{ALBUMS, TRACKS},
    controls::{heading, line, text},
    copy,
    player::duration,
};
use crate::theme::{ACCENT, ACTIVE, BG, MUTED, TEXT};
use lavashell::prelude::*;

fn track_header(ui: &mut Ui, compact: bool) {
    ui.row(
        Modifier::new()
            .gap(14.dp())
            .fill_max_width()
            .padding_xy(12.dp(), 6.dp()),
        |ui| {
            ui.spacer(24.dp(), 1.dp(), Modifier::new());
            ui.text(
                copy(CopyKey::TrackHeader),
                Modifier::new().font_size(10.sp()).color(MUTED).weight(2.0),
            );
            ui.text(
                copy(CopyKey::ArtistHeader),
                Modifier::new().font_size(10.sp()).color(MUTED).weight(1.0),
            );
            if !compact {
                ui.text(
                    copy(CopyKey::AlbumHeader),
                    Modifier::new().font_size(10.sp()).color(MUTED).weight(1.0),
                );
            }
            ui.text(
                copy(CopyKey::DurationHeader),
                Modifier::new()
                    .font_size(10.sp())
                    .color(MUTED)
                    .width(64.dp()),
            );
            ui.spacer(24.dp(), 1.dp(), Modifier::new());
        },
    );
}

/// What the track list shows when it has no track: an empty collection or a search without results.
fn empty_hint(ui: &mut Ui, m: &Model) {
    if m.query.is_empty() {
        text(ui, copy(CopyKey::CollectionEmpty), 15.sp(), TEXT);
        text(ui, copy(CopyKey::CollectionEmptyHint), 13.sp(), MUTED);
    } else {
        text(ui, copy(CopyKey::SearchEmpty), 15.sp(), TEXT);
        text(ui, copy(CopyKey::SearchHint), 13.sp(), MUTED);
    }
}

pub(super) fn track_list(ui: &mut Ui, model: &State<Model>, icons: Icons, compact: bool) {
    let m = model.get();
    let ids = m.visible();
    heading(ui, copy(CopyKey::Tracks), 19.sp());
    track_header(ui, compact);
    line(ui);

    if ids.is_empty() {
        empty_hint(ui, &m);
    }

    for index in ids {
        let track = &TRACKS[index];
        let album = &ALBUMS[track.album];
        let current = m.current == index;
        ui.key(index, Modifier::new(), |ui| {
            ui.row(
                Modifier::new()
                    .gap(14.dp())
                    .align_items(Align::Center)
                    .tag(format!("track-{index}"))
                    .accessible_name(track.title)
                    .selected(current)
                    .on_click(model.action(move |m| m.select(index)))
                    .fill_max_width()
                    .clip(8.dp())
                    .background(if current { ACTIVE } else { BG })
                    .padding_xy(12.dp(), 7.dp()),
                |ui| {
                    if current {
                        ui.icon(
                            icons.music,
                            IconOptions {
                                size: Some(14.dp()),
                                ..IconOptions::default()
                            },
                            Modifier::new().color(ACCENT).width(24.dp()),
                        );
                    } else {
                        ui.text(
                            format!("{:02}", index + 1),
                            Modifier::new()
                                .font_size(11.sp())
                                .color(MUTED)
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
                                    .color(if current { ACCENT } else { TEXT })
                                    .weight(1.0),
                            );
                        },
                    );
                    ui.text(
                        album.artist,
                        Modifier::new().font_size(12.sp()).color(MUTED).weight(1.0),
                    );
                    if !compact {
                        ui.text(
                            album.title,
                            Modifier::new().font_size(12.sp()).color(MUTED).weight(1.0),
                        );
                    }
                    ui.text(
                        duration(track.duration),
                        Modifier::new()
                            .font_size(11.sp())
                            .color(MUTED)
                            .width(64.dp()),
                    );
                    favorite_button(ui, model, icons.heart, index, m.favorites.contains(&index));
                },
            );
        });
    }
}

/// The button that adds the track at `index` to the favorites, or removes the track from the favorites when
/// `saved` is set.
fn favorite_button(
    ui: &mut Ui,
    model: &State<Model>,
    heart: &'static Icon,
    index: usize,
    saved: bool,
) {
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
            .color(if saved { ACCENT } else { MUTED })
            .padding(4.dp()),
        model.action(move |m| m.toggle_favorite(index)),
    );
}
