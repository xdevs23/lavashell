// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_playlist_wraps_around_at_both_ends() {
    let mut playlist = Playlist::default();
    playlist.previous();
    playlist.next();
    assert_eq!(playlist, Playlist::default());
    for _ in 0..TRACKS.len() {
        playlist.next();
    }
    assert_eq!(playlist, Playlist::default());
}
