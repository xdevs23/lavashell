// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use lavashell_core::geometry::{PixelScale, PixelSize, Size};
use lavashell_core::units::{Density, Dp, DpExt};
use std::time::Instant;

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

#[test]
fn a_composed_frame_goes_with_the_first_state_commit_after_it_and_no_later_one() -> TestResult {
    let viewport = Viewport::new(Size::new(40.dp(), 30.dp()), Density::ONE);
    let scaled_viewport = ScaledViewport {
        viewport,
        scale: PixelScale::filling(viewport, PixelSize::new(40, 30))?,
    };
    let timing = FrameTime {
        sequence: 7,
        ..FrameTime::default()
    };
    let mut surface = ViewSurface::new(None, 0);
    surface.frame = FrameProgress::Composed(FrameStart {
        wall: Instant::now(),
        view_cpu: None,
        timing,
        scaled_viewport,
    });
    let mut commit_state = || {
        surface.start_commit(None);
        surface.finish_commit().and_then(|commit| commit.composed)
    };
    let first = commit_state().map(|frame| (frame.timing.sequence, frame.viewport));
    assert_eq!(first, Some((7, viewport)));
    assert!(commit_state().is_none());
    Ok(())
}
