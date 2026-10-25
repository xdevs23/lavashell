// SPDX-License-Identifier: GPL-3.0-only
use super::{Rect, Transform, Unwind};
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

/// The transform that scales `bounds` by `scale` about its center, unless `scale` is one.
pub(in crate::view) fn about(bounds: Rect, scale: f64) -> Option<Transform> {
    ((scale - 1.0).abs() > f64::EPSILON).then(|| Transform::scaled_about(bounds, scale))
}

/// Scales the commands painted until `unwind` about the center of `bounds`.
pub(in crate::view) fn paint(canvas: &mut Canvas, scale: f64, bounds: Rect, unwind: &mut Unwind) {
    if let Some(next) = about(bounds, scale) {
        open(canvas, next, unwind);
    }
}

fn open(canvas: &mut Canvas, scale: Transform, unwind: &mut Unwind) {
    unwind.scope(canvas, ScopeKind::transform(scale.affine()));
}
