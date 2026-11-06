// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Effect, Merge};
use lavashell_core::scene::tree::{Change, ChangeEffect, ChangeKey};
use lavashell_core::surface::{SurfaceChange, SurfaceField};

/// A change of a scene or of the state of its surface, merged with the earlier items by its effect. A change of the
/// surface state overwrites the earlier change of the same part of the state.
pub(crate) enum SceneItem {
    Change(Change),
    Surface(SurfaceChange),
}

/// The value overwritten by an item.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SceneKey {
    Change(ChangeKey),
    Surface(SurfaceField),
}

impl Merge for SceneItem {
    type Key = SceneKey;

    fn effect(&self) -> Effect<SceneKey> {
        match self {
            Self::Change(change) => match change.effect() {
                ChangeEffect::Overwrites(key) => Effect::Overwrites(SceneKey::Change(key)),
                ChangeEffect::BuildsOn => Effect::BuildsOn,
                ChangeEffect::Fences(key) => Effect::Fences(SceneKey::Change(key)),
            },
            Self::Surface(change) => Effect::Overwrites(SceneKey::Surface(change.field())),
        }
    }
}
