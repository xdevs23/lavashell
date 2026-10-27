// SPDX-License-Identifier: GPL-3.0-only
use super::{ModifierElement, Point, Rect, Transform, View};
use crate::effects::{Material, MaterialCommands, ShapedMaterial};
use crate::shape::{RoundedRect, Shape};
use crate::units::NonNegativeDp;
use crate::view::output::Painted;
use crate::view::shown_children;

/// The drawing commands of `shaped` on the surface `rect`. For a node in a transition, the material is shaped at the
/// animated bounds, to which `stretch` maps `rect`. The commands are then mapped back by the inverse of `stretch`,
/// because the node paints them under a transform that contains `stretch`.
pub(in crate::view) fn commands(
    rect: Rect,
    shaped: &ShapedMaterial,
    stretch: Transform,
) -> MaterialCommands {
    // Without the translation, the commands are the same while only the position of the node animates.
    let stretch = Transform {
        translation: Point::default(),
        ..stretch
    };
    let rect = stretch.rect(rect);
    let shape = RoundedRect::new(rect, shaped.corners).into();
    let painted = (shaped.material).commands(&shape, Point::new(rect.x, rect.y));
    painted.transformed(stretch.inverse().affine())
}

/// A material group of a node, painted once the islands among the children of the node have their bounds of this
/// frame.
pub(super) struct Group {
    pub material: Material,
    pub spacing: NonNegativeDp,
    /// The point that the brushes of the material are placed relative to.
    pub origin: Point,
}

impl View {
    /// Paints each material group of the node at `index` as one shape that joins the islands among the shown children
    /// of the node, below `content`. The children paint before this call, so that the bounds of the islands are up to
    /// date for this frame.
    pub(super) fn paint_groups(
        &mut self,
        index: usize,
        groups: Vec<Group>,
        transform: Transform,
        content: Vec<Painted>,
    ) {
        let local = transform.inverse();
        let islands: Vec<_> = shown_children(&self.nodes, &self.layout, index)
            .filter_map(|(_, child)| {
                let corners = self.nodes[child].modifier.iter().find_map(|e| match e {
                    ModifierElement::Island(corners) => Some(*corners),
                    _ => None,
                })?;
                Some(RoundedRect::new(
                    local.rect(self.painted_bounds(child)),
                    corners,
                ))
            })
            .collect();
        let (mut below, mut unwind) = (Vec::new(), Vec::new());
        for group in groups {
            let members = islands.iter().map(|&island| island.into());
            if let Some(shape) = Shape::union(members, group.spacing) {
                let commands = group.material.commands(&shape, group.origin);
                commands.paint(&mut below, &mut unwind);
            }
        }
        self.canvas.commands().append(&mut below);
        let appended = self.canvas.append(content);
        self.canvas.commands().extend(unwind.into_iter().rev());
        if let Err(error) = appended {
            self.fail_paint(error);
        }
    }
}
