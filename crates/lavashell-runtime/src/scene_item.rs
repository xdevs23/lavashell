// SPDX-License-Identifier: GPL-3.0-only
use crate::scheduler::FrameSequence;
use lavashell_actor::{Frame, Merge, MergeEffect};
use lavashell_core::scene::tree::{Change, ChangeEffect, ChangeKey};
use lavashell_core::surface::{SurfaceChange, SurfaceField};

pub(crate) enum SceneItem {
    Change(Change),
    Surface(SurfaceChange),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SceneKey {
    Change(ChangeKey),
    Surface(SurfaceField),
}

pub(crate) type SceneFrame = Frame<SceneItem, FrameSequence>;

impl Merge for SceneItem {
    type Key = SceneKey;

    fn effect(&self) -> MergeEffect<SceneKey> {
        match self {
            Self::Change(change) => match change.effect() {
                ChangeEffect::Overwrites(key) => MergeEffect::Overwrites(SceneKey::Change(key)),
                ChangeEffect::BuildsOn => MergeEffect::Appends,
                ChangeEffect::Fences(key) => MergeEffect::Fences(SceneKey::Change(key)),
            },
            Self::Surface(change) => MergeEffect::Overwrites(SceneKey::Surface(change.field())),
        }
    }
}
