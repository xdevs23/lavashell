// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn compass_direction(edge: ResizeEdge) -> String {
    format!("{edge:?}")
        .replace("Top", "n")
        .replace("Bottom", "s")
        .replace("Right", "e")
        .replace("Left", "w")
}

#[test]
fn every_resize_edge_shows_the_resize_cursor_of_its_compass_direction() {
    for edge in ResizeEdge::ALL {
        let icon = cursor_icon(Cursor::Resize(edge));
        assert_eq!(icon.name(), format!("{}-resize", compass_direction(edge)));
    }
}
