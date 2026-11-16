// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::board::PADDING;
use crate::look::Surface;

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
    let snapshot = scene.builder.snapshot();
    (snapshot.at(scene.now).paint_order().leaves())
        .map(|leaf| (leaf.world, leaf.packet.commands().to_vec()))
        .collect()
}

pub(super) fn texts(scene: &Scene) -> Vec<String> {
    let snapshot = scene.builder.snapshot();
    (snapshot.at(scene.now).leaf_commands())
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

pub(super) fn rings(scene: &Scene) -> Vec<Rect> {
    let accent = KeyboardStyle::night()
        .looks(KEY_HEIGHT)
        .get(Surface::Glass)
        .accent;
    painted(scene, &|command| match command {
        DrawCommand::Stroke { shape, stroke } if stroke.color == accent => Some(shape.bounds()),
        _ => None,
    })
}

pub(super) fn painted(scene: &Scene, select: &dyn Fn(&DrawCommand) -> Option<Rect>) -> Vec<Rect> {
    let mut found = Vec::new();
    for (world, commands) in leaves(scene) {
        walk(&commands, world, select, &mut found);
    }
    found
}

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
