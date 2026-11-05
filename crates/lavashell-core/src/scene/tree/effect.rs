// SPDX-License-Identifier: GPL-3.0-only
use super::{Change, Content, SceneNodeId, SlotChange};

/// What a change does to the changes sent before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeEffect {
    /// The change sets the whole value of one field of one node, so it replaces the earlier change of that field that
    /// no fence of that field follows. It applies in any order with the changes of other nodes and with moves of its
    /// node.
    Overwrites(ChangeKey),
    /// The change depends on the changes before it, because it adds, removes or moves a node.
    BuildsOn,
    /// The change grafts a namespace as the content of a node. It depends on the changes before it, and a later change
    /// of the same field applies after it.
    Fences(ChangeKey),
}

/// One field of one node.
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
