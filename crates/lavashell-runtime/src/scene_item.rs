// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameSequence;
use lavashell_actor::{Effect, Frame, Merge};
use lavashell_core::scene::tree::{Change, ChangeEffect, ChangeKey};

/// A change of a scene, merged with the earlier changes by the effect of the change.
pub(crate) struct SceneItem(pub(crate) Change);

/// The changes that the choreographer merged for a surface, ending with the sequence of the newest composed frame.
pub(crate) type SceneFrame = Frame<SceneItem, FrameSequence>;

impl Merge for SceneItem {
    type Key = ChangeKey;

    fn effect(&self) -> Effect<ChangeKey> {
        match self.0.effect() {
            ChangeEffect::Overwrites(key) => Effect::Overwrites(key),
            ChangeEffect::BuildsOn => Effect::BuildsOn,
            ChangeEffect::Fences(key) => Effect::Fences(key),
        }
    }
}
