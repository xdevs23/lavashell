// SPDX-License-Identifier: GPL-3.0-only
use crate::effects::{Material, ShapedMaterial};
use crate::geometry::{Point, Rect};
use crate::scene::{MaterialCommands, Transform};
use crate::shape::{RoundedRect, Shape};
use crate::ui::ModifierElement;
use crate::units::NonNegativeDp;
use crate::view::View;
use crate::view::layout::shown_children;
use crate::view::output::Painted;

pub(in crate::view) fn commands(
    rect: Rect,
    shaped: &ShapedMaterial,
    stretch: Transform,
) -> MaterialCommands {
    let stretch = Transform {
        translation: Point::default(),
        ..stretch
    };
    let rect = stretch.rect(rect);
    let shape = RoundedRect::new(rect, shaped.corners).into();
    let painted = MaterialCommands::of(&shaped.material, &shape, Point::new(rect.x, rect.y));
    painted.transformed(stretch.inverse().affine())
}

pub(super) struct Group {
    pub material: Material,
    pub spacing: NonNegativeDp,
    pub origin: Point,
}

impl View {
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
                let commands = MaterialCommands::of(&group.material, &shape, group.origin);
                commands.paint(&mut below, &mut unwind);
            }
        }
        self.painting.canvas.commands().append(&mut below);
        self.painting.canvas.append(content);
        self.painting
            .canvas
            .commands()
            .extend(unwind.into_iter().rev());
    }
}
