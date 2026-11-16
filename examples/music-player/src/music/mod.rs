// SPDX-License-Identifier: GPL-3.0-only
mod art;
mod catalog;
mod controls;
mod icons;
mod library;
mod model;
mod player;
mod screen;
mod tag;
mod tracks;

pub use catalog::{AlbumId, Playlist, TrackId};
pub use icons::Icons;
pub use library::music;
pub use model::{Model, Section};
pub use player::{PLAYER_HEIGHT, panel};
pub use tag::Tag;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));
