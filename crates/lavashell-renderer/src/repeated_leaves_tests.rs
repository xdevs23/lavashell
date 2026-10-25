// SPDX-License-Identifier: GPL-3.0-only
use crate::packet_cache::PacketCacheKey;
use crate::repeated_leaves::RepeatedLeafFilter;
use lavashell_core::scene::tree::{NodeKey, Packet, SceneNodeId, SceneTree};
use lavashell_core::surface::SurfaceId;
use skia_safe::Matrix;
use std::collections::HashSet;
use std::sync::Arc;

fn leaf(node: u64) -> NodeKey {
    NodeKey::new(SceneTree::ROOT, SceneNodeId(node))
}

fn packet() -> Packet {
    Packet::new(Arc::from([]))
}

fn repeats(filter: &mut RepeatedLeafFilter, packet: &Packet, surface: u64, node: u64) -> bool {
    let key = PacketCacheKey::new(
        SurfaceId(surface),
        leaf(node),
        packet,
        &Matrix::new_identity(),
    );
    filter.repeats(key)
}

#[test]
fn a_leaf_repeats_at_its_second_plan() {
    let (mut filter, packet) = (RepeatedLeafFilter::default(), packet());
    assert!(!repeats(&mut filter, &packet, 1, 1));
    assert!(repeats(&mut filter, &packet, 1, 1));
}

#[test]
fn a_leaf_with_a_new_packet_starts_over() {
    let mut filter = RepeatedLeafFilter::default();
    assert!(!repeats(&mut filter, &packet(), 1, 1));
    assert!(!repeats(&mut filter, &packet(), 1, 1));
}

#[test]
fn a_leaf_that_a_plan_no_longer_shows_starts_over() {
    let (mut filter, packet) = (RepeatedLeafFilter::default(), packet());
    assert!(!repeats(&mut filter, &packet, 1, 1));
    filter.retain_shown(SurfaceId(1), &HashSet::new());
    assert!(!repeats(&mut filter, &packet, 1, 1));
}

#[test]
fn the_history_of_a_shown_leaf_and_of_other_surfaces_remains() {
    let (mut filter, packet) = (RepeatedLeafFilter::default(), packet());
    assert!(!repeats(&mut filter, &packet, 1, 1));
    assert!(!repeats(&mut filter, &packet, 2, 1));
    filter.retain_shown(SurfaceId(1), &HashSet::from([leaf(1)]));
    filter.retain_shown(SurfaceId(2), &HashSet::from([leaf(9)]));
    assert!(repeats(&mut filter, &packet, 1, 1));
    assert!(!repeats(&mut filter, &packet, 2, 1));
    filter.retain_shown(SurfaceId(1), &HashSet::from([leaf(1)]));
    assert!(repeats(&mut filter, &packet, 1, 1));
}
