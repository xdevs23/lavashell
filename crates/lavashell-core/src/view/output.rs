// SPDX-License-Identifier: GPL-3.0-only
mod canvas;
#[cfg(test)]
mod canvas_tests;
mod shown;

pub(super) use canvas::{Canvas, ScopeKind};
pub(super) use shown::{Recipient, Shown};

use crate::geometry::Point;
use crate::scene::tree::{Group, GroupValues, NamespaceId};
use crate::scene::{Affine, DrawCommand};
use crate::ui::NodeId;
use std::convert::Infallible;
use std::sync::Arc;

/// An item painted by the view. A graft shows a `G`, which is a namespace, or `Infallible` inside a scope painted as
/// drawing commands, because such a scope cannot contain a namespace.
pub(super) enum Painted<G = NamespaceId> {
    Leaf {
        commands: Arc<[DrawCommand]>,
    },
    Scope {
        values: GroupValues,
        items: Vec<Painted<G>>,
    },
    Group {
        id: GroupId,
        group: Group,
        items: Arc<[Painted<G>]>,
    },
    /// A leaf that shows the root of a namespace of the tree that shows the view.
    Graft(G),
}

/// An item of a scope painted as drawing commands.
type Flat = Painted<Infallible>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum GroupId {
    Component(NodeId),
    Presence(NodeId),
    /// The part of a node after the animated modifier at `step` of its modifier chain.
    Animated {
        node: NodeId,
        step: usize,
    },
    Content(NodeId),
}

impl Painted {
    /// This item for a scope painted as drawing commands, or the first namespace grafted in it.
    fn flat(&self) -> Result<Flat, NamespaceId> {
        Ok(match self {
            Self::Leaf { commands } => Flat::Leaf {
                commands: commands.clone(),
            },
            Self::Scope { values, items } => Flat::Scope {
                values: *values,
                items: items.iter().map(Self::flat).collect::<Result<_, _>>()?,
            },
            Self::Group { id, group, items } => Flat::Group {
                id: *id,
                group: group.clone(),
                items: items.iter().map(Self::flat).collect::<Result<_, _>>()?,
            },
            Self::Graft(namespace) => return Err(*namespace),
        })
    }
}

impl Flat {
    fn flatten_into(&self, out: &mut Vec<DrawCommand>) {
        match self {
            Self::Leaf { commands } => out.extend(commands.iter().cloned()),
            Self::Scope { values, items } => flatten_group(*values, out, |out| flatten(items, out)),
            Self::Group { group, items, .. } => {
                flatten_group(group.end(), out, |out| flatten(items, out));
            }
            Self::Graft(never) => match *never {},
        }
    }
}

pub(super) fn placed_at(position: Point) -> Group {
    GroupValues {
        transform: Affine::translate(position),
        ..GroupValues::default()
    }
    .into()
}

pub(super) fn flatten_group(
    values: GroupValues,
    out: &mut Vec<DrawCommand>,
    paint: impl FnOnce(&mut Vec<DrawCommand>),
) {
    let restores = values.open(out);
    paint(out);
    restores.close(out);
}

fn flatten(items: &[Flat], out: &mut Vec<DrawCommand>) {
    for item in items {
        item.flatten_into(out);
    }
}
