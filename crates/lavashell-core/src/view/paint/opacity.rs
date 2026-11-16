// SPDX-License-Identifier: GPL-3.0-only
use super::Unwind;
use crate::units::Fraction;

use crate::view::output::{Canvas, ScopeKind};

pub(in crate::view) fn paint(canvas: &mut Canvas, alpha: Fraction, unwind: &mut Unwind) {
    unwind.scope(canvas, ScopeKind::opacity(alpha.get()));
}
