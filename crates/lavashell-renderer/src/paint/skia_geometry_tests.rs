// SPDX-License-Identifier: GPL-3.0-only
use super::{clip_device, skpath_through};
use crate::test_error::TestError;
use crate::test_surfaces::{pixels, surface};
use lavashell_core::prelude::*;
use skia_safe::{Contains, IPoint, IRect, Paint, PathVerb, Rect as SkRect};

const SIDE: i32 = 8;
const CANVAS_SCALE: f32 = 2.0;

#[test]
fn a_path_keeps_its_curves_and_closes_only_when_asked() -> Result<(), TestError> {
    let at = |x: f64, y: f64| Point::new(Dp::new(x), Dp::new(y));
    let segments = [
        Segment::Line(at(10.0, 0.0)),
        Segment::Quadratic {
            control: at(20.0, 0.0),
            to: at(20.0, 10.0),
        },
        Segment::Cubic {
            first_control: at(20.0, 20.0),
            second_control: at(10.0, 20.0),
            to: at(0.0, 10.0),
        },
    ];
    let open = skpath_through(at(0.0, 0.0), &segments, false)?;
    assert_eq!(
        open.verbs(),
        [
            PathVerb::Move,
            PathVerb::Line,
            PathVerb::Quad,
            PathVerb::Cubic
        ]
    );
    let closed = skpath_through(at(0.0, 0.0), &segments, true)?;
    assert_eq!(closed.verbs().last(), Some(&PathVerb::Close));
    Ok(())
}

/// The row-major mask of the pixels that turn opaque when the surface is filled after the clip to `clip`.
fn opaque_mask_after_clip(clip: IRect) -> Result<Vec<bool>, TestError> {
    let mut target = surface(SIDE, SIDE)?;
    let canvas = target.canvas();
    let _chained_canvas = canvas.scale((CANVAS_SCALE, CANVAS_SCALE));
    clip_device(canvas, clip);
    let everything = SkRect::from_irect(IRect::from_wh(SIDE, SIDE));
    let _chained_canvas = canvas.draw_rect(everything, &Paint::default());
    let bytes = pixels(&mut target);
    let (texels, _) = bytes.as_chunks::<4>();
    Ok(texels.iter().map(|texel| texel[3] > 0).collect())
}

/// The row-major mask of the pixels of the surface inside `area`.
fn mask_of_pixels_inside(area: IRect) -> Vec<bool> {
    let contains = |x, y| area.contains(IPoint::new(x, y));
    (0..SIDE)
        .flat_map(|y| (0..SIDE).map(move |x| contains(x, y)))
        .collect()
}

#[test]
fn a_canvas_clipped_to_device_pixels_paints_only_them() -> Result<(), TestError> {
    let clip = IRect::new(1, 2, 5, 7);
    assert_eq!(opaque_mask_after_clip(clip)?, mask_of_pixels_inside(clip));
    Ok(())
}
