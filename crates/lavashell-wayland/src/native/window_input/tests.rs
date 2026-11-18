// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn every_resize_edge_becomes_the_xdg_toplevel_edge_of_the_same_name() {
    for edge in ResizeEdge::ALL {
        let native = native_resize_edge(edge);
        assert_eq!(format!("{native:?}"), format!("{edge:?}"));
    }
}
