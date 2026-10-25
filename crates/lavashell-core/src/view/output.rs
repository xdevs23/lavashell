// SPDX-License-Identifier: GPL-3.0-only
mod canvas;
#[cfg(test)]
mod canvas_tests;
mod shown;

pub(super) use canvas::{Canvas, ScopeKind};
pub(super) use shown::{Recipient, Shown};

use crate::geometry::Point;
use crate::scene::tree::{Group, GroupValues};
use crate::scene::{Affine, DrawCommand};
use crate::ui::NodeId;
use std::sync::Arc;

pub(super) enum Painted {
    Leaf {
        commands: Arc<[DrawCommand]>,
    },
    Scope {
        values: GroupValues,
        items: Vec<Painted>,
    },
    Group {
        id: GroupId,
        group: Group,
        items: Arc<[Painted]>,
    },
}

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
    fn flatten_into(&self, out: &mut Vec<DrawCommand>) {
        match self {
            Self::Leaf { commands } => out.extend(commands.iter().cloned()),
            Self::Scope { values, items } => flatten_group(*values, out, |out| flatten(items, out)),
            Self::Group { group, items, .. } => {
                flatten_group(group.end(), out, |out| flatten(items, out));
            }
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

fn flatten(items: &[Painted], out: &mut Vec<DrawCommand>) {
    for item in items {
        item.flatten_into(out);
    }
}
