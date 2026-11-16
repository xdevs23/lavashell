// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, Presented};
use crate::scene::{Affine, DrawCommand};

pub(crate) struct Restores(usize);

impl Restores {
    const NONE: Self = Self(0);

    pub(crate) fn close(self, out: &mut Vec<DrawCommand>) {
        out.extend(std::iter::repeat_n(DrawCommand::Restore, self.0));
    }
}

impl GroupValues {
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
