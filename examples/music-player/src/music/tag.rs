// SPDX-License-Identifier: GPL-3.0-only
use super::{CopyKey, catalog::TrackId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    CollectionScroll,
    Hero,
    SearchInput,
    PlayAlbum,
    PlayToggle,
    SaveCurrent,
    Shuffle,
    Previous,
    Next,
    Repeat,
    Seek,
    Volume,
    Navigation(CopyKey),
    Track(TrackId),
}
