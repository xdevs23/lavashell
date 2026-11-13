// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::units::DpExt;

#[test]
fn a_surface_size_is_at_least_one_logical_pixel_per_length() {
    let size = |width: Dp, height: Dp| surface_size(Size::new(width, height));
    assert_eq!(size(0.3.dp(), 0.3.dp()), Some((1, 1)));
    assert_eq!(size(61.6.dp(), 100.dp()), Some((62, 100)));
    assert_eq!(size(-1.dp(), 100.dp()), None);
    assert_eq!(size(100.dp(), 3e9.dp()), None);
    assert_eq!(size(Dp::INFINITY, 100.dp()), None);
}
