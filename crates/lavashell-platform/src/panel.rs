// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::surface::SurfaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelRequest {
    SetKeyboard(SurfaceId, KeyboardMode),
}

impl Merge for PanelRequest {
    type Key = SurfaceId;

    fn effect(&self) -> MergeEffect<SurfaceId> {
        match *self {
            Self::SetKeyboard(surface, _) => MergeEffect::Overwrites(surface),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    Off,
    OnDemand,
}
