// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Effect, Merge};
use lavashell_core::scene::tree::{Change, ChangeEffect, ChangeKey};

/// A change of a scene, merged with the earlier changes by the effect of the change.
pub(crate) struct SceneItem(pub(crate) Change);

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
