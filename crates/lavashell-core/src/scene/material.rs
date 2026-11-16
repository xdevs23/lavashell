// SPDX-License-Identifier: GPL-3.0-only
use super::{Affine, DrawCommand};
use crate::effects::Material;
use crate::geometry::Point;
use crate::shape::Shape;

pub(crate) struct MaterialCommands {
    below: Vec<DrawCommand>,
    enclosing: Vec<DrawCommand>,
}

impl MaterialCommands {
    pub fn of(material: &Material, shape: &Shape, origin: Point) -> Self {
        let shadows = |inset: bool| {
            material
                .shadows
                .iter()
                .filter(move |s| s.inset == inset)
                .map(|shadow| DrawCommand::Shadow {
                    shape: shape.clone(),
                    shadow: *shadow,
                })
        };
        let backdrop = material.backdrop.map(|effect| DrawCommand::Backdrop {
            shape: shape.clone(),
            effect,
        });
        let fills = std::iter::once(&material.fill)
            .chain(&material.shading)
            .map(|brush| DrawCommand::Fill {
                shape: shape.clone(),
                brush: brush.clone().translated(origin),
            });
        let border = material.border.map(|stroke| DrawCommand::Stroke {
            shape: shape.clone(),
            stroke,
        });
        Self {
            below: shadows(false).chain(backdrop).chain(fills).collect(),
            enclosing: shadows(true).chain(border).collect(),
        }
    }

    pub fn transformed(self, transform: Affine) -> Self {
        if transform == Affine::IDENTITY {
            return self;
        }
        let wrap = |inner: Vec<DrawCommand>| {
            if inner.is_empty() {
                return inner;
            }
            let open = [DrawCommand::Save, DrawCommand::Transform(transform)];
            open.into_iter()
                .chain(inner)
                .chain([DrawCommand::Restore])
                .collect()
        };
        Self {
            below: wrap(self.below),
            enclosing: wrap(self.enclosing),
        }
    }

    pub fn paint(self, commands: &mut Vec<DrawCommand>, unwind: &mut impl Extend<DrawCommand>) {
        commands.extend(self.below);
        unwind.extend(self.enclosing.into_iter().rev());
    }
}
