// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use lavashell_core::geometry::{PixelScale, PixelSize, Size};
use lavashell_core::units::{Density, Dp, DpExt};

#[test]
fn contents_drawn_at_one_scale_are_outdated_at_another_scale_of_the_same_buffer() -> TestResult {
    let viewport = Viewport::new(Size::new(40.dp(), 30.dp()), Density::ONE);
    let buffer = PixelSize::new(64, 30);
    let drawn = ScaledViewport {
        viewport,
        scale: PixelScale::filling(viewport, buffer)?,
    };
    let wider = Viewport::new(Size::new(Dp::new(40.1), 30.dp()), Density::ONE);
    let rescaled = ScaledViewport {
        scale: PixelScale::filling(wider, buffer)?,
        ..drawn
    };
    assert_eq!(rescaled.buffer_size()?, drawn.buffer_size()?);
    let mut surface = ViewSurface::new(None, 0);
    let _no_frame_was_drawing =
        surface.finish_drawing(drawn, SurfaceContents::Current, FrameTime::default());
    assert_eq!(surface.contents(drawn), SurfaceContents::Current);
    assert_eq!(surface.contents(rescaled), SurfaceContents::Outdated);
    Ok(())
}
