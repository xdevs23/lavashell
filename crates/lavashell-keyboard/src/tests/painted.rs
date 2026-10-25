// SPDX-License-Identifier: GPL-3.0-only
//! Functions that read the draw commands of a test scene and return the bounds on the screen of the
//! shapes they select.
use super::*;
use crate::board::PADDING;

/// The rounded fills as tall as a key and as wide as the panel inside its padding. They are the rows
/// of keys drawn as one shape each.
pub(super) fn fused_rows(scene: &Scene) -> Vec<Rect> {
    let width = SCREEN.width - PADDING.left - PADDING.right;
    painted(scene, &|command| match command {
        DrawCommand::Fill {
            shape: Shape::Rounded(shape),
            ..
        } if (shape.rect.width - width).abs() < 0.5.dp() && shape.rect.height == KEY_HEIGHT => {
            Some(shape.rect)
        }
        _ => None,
    })
}

/// The bounds of every union shape with exactly `members` members, such as the bubble and the key of
/// a preview drawn as one shape.
pub(super) fn unions(scene: &Scene, members: usize) -> Vec<Rect> {
    painted(scene, &|command| match command {
        DrawCommand::Fill {
            shape: Shape::Union(union),
            ..
        } if union.members().len() == members => Some(union.bounds()),
        _ => None,
    })
}

pub(super) fn leaves(scene: &Scene) -> Vec<(Affine, Vec<DrawCommand>)> {
    (scene.tree.at(scene.now).paint_order().leaves())
        .map(|leaf| (leaf.world, leaf.packet.commands().to_vec()))
        .collect()
}

pub(super) fn texts(scene: &Scene) -> Vec<String> {
    (scene.tree.at(scene.now).leaf_commands())
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn backdrops(scene: &Scene) -> Vec<Rect> {
    painted(scene, &|command| match command {
        DrawCommand::Backdrop { shape, .. } => Some(shape.bounds()),
        _ => None,
    })
}

/// The bounds of every stroke in the accent color of the night style, which is the focus ring.
pub(super) fn rings(scene: &Scene) -> Vec<Rect> {
    let accent = KeyboardStyle::night().looks(KEY_HEIGHT).get(false).accent;
    painted(scene, &|command| match command {
        DrawCommand::Stroke { shape, stroke } if stroke.color == accent => Some(shape.bounds()),
        _ => None,
    })
}

/// The bounds returned by `select` for the draw commands of `scene`, mapped to the screen by the
/// transforms that apply to each command.
pub(super) fn painted(scene: &Scene, select: &dyn Fn(&DrawCommand) -> Option<Rect>) -> Vec<Rect> {
    let mut found = Vec::new();
    for (world, commands) in leaves(scene) {
        walk(&commands, world, select, &mut found);
    }
    found
}

/// Adds to `found` the bounds returned by `select` for `commands`, each mapped by the transform that
/// applies to its command, starting from `world`. A restore without a matching save changes nothing,
/// as on a canvas.
fn walk(
    commands: &[DrawCommand],
    world: Affine,
    select: &dyn Fn(&DrawCommand) -> Option<Rect>,
    found: &mut Vec<Rect>,
) {
    let (mut current, mut saved) = (world, Vec::new());
    for command in commands {
        match command {
            DrawCommand::Save
            | DrawCommand::Opacity(_)
            | DrawCommand::OpacityLayer { .. }
            | DrawCommand::EffectLayer { .. } => saved.push(current),
            DrawCommand::Restore => {
                if let Some(outer) = saved.pop() {
                    current = outer;
                }
            }
            DrawCommand::Transform(inner) => current = current.then(*inner),
            command => found.extend(select(command).map(|bounds| map(current, bounds))),
        }
    }
}

fn map(affine: Affine, rect: Rect) -> Rect {
    let start = affine.map_point(Point::new(rect.x, rect.y));
    let end = affine.map_point(Point::new(rect.x + rect.width, rect.y + rect.height));
    Rect::new(start.x, start.y, end.x - start.x, end.y - start.y)
}
