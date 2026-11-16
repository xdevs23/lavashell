// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_playlist_wraps_around_at_both_ends() {
    let mut playlist = Playlist::Undertow;
    playlist.previous();
    assert_eq!(playlist, Playlist::Glasshouse);
    playlist.next();
    assert_eq!(playlist, Playlist::Undertow);
    playlist.next();
    assert_eq!(playlist, Playlist::HarborLights);
    playlist.next();
    playlist.next();
    assert_eq!(playlist, Playlist::Undertow);
}
