// SPDX-License-Identifier: GPL-3.0-only
use super::Unwind;
use crate::geometry::Rect;
use crate::scene::Transform;
use crate::view::output::{Canvas, ScopeKind};

pub(super) fn apply(
    canvas: &mut Canvas,
    scale: f64,
    bounds: Rect,
    transform: &mut Transform,
    local: &mut Transform,
    unwind: &mut Unwind,
) {
    if let Some(next) = about(bounds, scale) {
        open(canvas, next, unwind);
        *transform = transform.then(next);
        *local = local.then(next);
    }
}

pub(in crate::view) fn about(bounds: Rect, scale: f64) -> Option<Transform> {
    ((scale - 1.0).abs() > f64::EPSILON).then(|| Transform::scaled_about(bounds, scale))
}

pub(in crate::view) fn paint(canvas: &mut Canvas, scale: f64, bounds: Rect, unwind: &mut Unwind) {
    if let Some(next) = about(bounds, scale) {
        open(canvas, next, unwind);
    }
}

fn open(canvas: &mut Canvas, scale: Transform, unwind: &mut Unwind) {
    unwind.scope(canvas, ScopeKind::transform(scale.affine()));
}
