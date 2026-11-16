// SPDX-License-Identifier: GPL-3.0-only
use super::{Change, Content, SceneNodeId, SlotChange};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeEffect {
    Overwrites(ChangeKey),
    BuildsOn,
    Fences(ChangeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeKey {
    pub id: SceneNodeId,
    pub field: ChangeField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeField {
    Content,
    Transform,
    Opacity,
    Clip,
}

impl Change {
    #[must_use]
    pub fn effect(&self) -> ChangeEffect {
        match *self {
            Self::SetContent { id, ref content } => {
                let key = ChangeKey {
                    id,
                    field: ChangeField::Content,
                };
                match content {
                    Content::Packet(_) => ChangeEffect::Overwrites(key),
                    Content::Graft(_) => ChangeEffect::Fences(key),
                }
            }
            Self::SetSlot { id, ref slot } => ChangeEffect::Overwrites(ChangeKey {
                id,
                field: slot.field(),
            }),
            Self::Insert { .. } | Self::Remove { .. } | Self::Move { .. } => ChangeEffect::BuildsOn,
        }
    }
}

impl SlotChange {
    fn field(&self) -> ChangeField {
        match self {
            Self::Transform(_) => ChangeField::Transform,
            Self::Opacity(_) => ChangeField::Opacity,
            Self::Clip(_) => ChangeField::Clip,
        }
    }
}
