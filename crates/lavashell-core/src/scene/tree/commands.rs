// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, Presented};
use crate::scene::{Affine, DrawCommand};

/// The number of `Restore` commands that undo the `Save` and `Opacity` commands pushed by `GroupValues::open`.
pub(crate) struct Restores(usize);

impl Restores {
    const NONE: Self = Self(0);

    pub(crate) fn close(self, out: &mut Vec<DrawCommand>) {
        out.extend(std::iter::repeat_n(DrawCommand::Restore, self.0));
    }
}

impl GroupValues {
    /// Pushes the commands that apply the values of a group to the commands after them: the transform, then the clip
    /// in the transformed coordinates, then the opacity.
    pub(crate) fn open(self, out: &mut Vec<DrawCommand>) -> Restores {
        let Self {
            transform,
            opacity,
            clip,
        } = self;
        out.push(DrawCommand::Save);
        if transform != Affine::IDENTITY {
            out.push(DrawCommand::Transform(transform));
        }
        if let Some(clip) = clip {
            out.push(DrawCommand::Clip {
                rect: clip.rect,
                corners: clip.corners.into(),
            });
        }
        if opacity < 1.0 {
            out.push(DrawCommand::Opacity(opacity));
            return Restores(2);
        }
        Restores(1)
    }
}

impl Presented<'_> {
    /// The draw commands of the whole tree in paint order. The commands of the children of a group sit between the
    /// commands that apply the values of the group and the matching `Restore` commands. A graft adds no commands
    /// around the namespace it grafts.
    #[must_use]
    pub fn commands(self) -> Vec<DrawCommand> {
        let mut flattened = Flattened(Vec::new());
        self.paint_order().walk(&mut flattened);
        flattened.0
    }
}

struct Flattened(Vec<DrawCommand>);

impl PaintVisitor<'_> for Flattened {
    type Saved = Restores;

    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) -> Restores {
        values.open(&mut self.0)
    }

    fn graft(&mut self, _: NodeKey, _: NamespaceId) -> Restores {
        Restores::NONE
    }

    fn leaf(&mut self, leaf: PlacedLeaf<'_>) {
        self.0.extend(leaf.packet.commands().iter().cloned());
    }

    fn exit(&mut self, restores: Restores) {
        restores.close(&mut self.0);
    }
}
