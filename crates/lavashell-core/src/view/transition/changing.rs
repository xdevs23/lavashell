// SPDX-License-Identifier: GPL-3.0-only
use super::{Point, Presence, Rect, Visual, VisualVectors};
use crate::animation::{Animatable, Program, Vector, VectorSpace};
use crate::scene::tree::PresenceVectors;

#[derive(Clone, Copy)]
pub(in crate::view) struct Changing {
    /// Whether the motion changes the position of the bounds, or the offset or the scale of the
    /// presence.
    pub moves: bool,
    pub fades: bool,
    /// Whether the motion changes the size of the bounds or the appearance. The view reads those
    /// values while it paints the node, so the scene cannot play such a motion in its slots.
    pub reshapes: bool,
}

impl Changing {
    pub(super) fn of(program: &Program<Visual>) -> Option<Self> {
        let Program::Target {
            from, to, velocity, ..
        } = program
        else {
            return None;
        };
        let [from, to, velocity] = [from, to, velocity].map(VisualVectors::of);
        let [presence_from, presence_to, presence_velocity] =
            [from.presence, to.presence, velocity.presence].map(PresenceVectors::of);
        let changes = |from: &[f64], to: &[f64], velocity: &[f64]| {
            from != to || velocity.iter().any(|v| *v != 0.0)
        };
        let rect = |part: fn(&Vector<4>) -> [f64; 2]| {
            changes(&part(from.rect), &part(to.rect), &part(velocity.rect))
        };
        let reshapes = rect(Rect::vector_size)
            || from.appearance != to.appearance
            || velocity.appearance.max_abs() != 0.0;
        let moves = rect(Rect::vector_position)
            || changes(
                &presence_from.offset.0,
                &presence_to.offset.0,
                &presence_velocity.offset.0,
            )
            || changes(
                &presence_from.scale.0,
                &presence_to.scale.0,
                &presence_velocity.scale.0,
            );
        let fades = changes(
            &presence_from.opacity.0,
            &presence_to.opacity.0,
            &presence_velocity.opacity.0,
        );
        Some(Self {
            moves,
            fades,
            reshapes,
        })
    }
}

impl VisualVectors<'_> {
    pub fn presenting(&self) -> <(Point, Presence) as Animatable>::Vector {
        (Vector(Rect::vector_position(self.rect)), *self.presence)
    }
}
