// SPDX-License-Identifier: GPL-3.0-only

use num_traits::AsPrimitive;

pub(crate) fn nearest(value: f64) -> Option<f32> {
    let narrow: f32 = value.as_();
    (narrow.is_finite() || !value.is_finite()).then_some(narrow)
}

#[cfg(test)]
mod tests;
