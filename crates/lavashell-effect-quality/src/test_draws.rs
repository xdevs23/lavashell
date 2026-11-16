// SPDX-License-Identifier: GPL-3.0-only
use crate::{DrawId, EffectId};
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceId;

pub(crate) fn draw_id(ordinal: u64) -> DrawId {
    draw_on(SurfaceId::numbered(0), ordinal)
}

pub(crate) fn draw_on(surface: SurfaceId, ordinal: u64) -> DrawId {
    DrawId {
        surface,
        leaf: SceneTree::LEAF,
        ordinal,
        effect: EffectId(0),
    }
}
