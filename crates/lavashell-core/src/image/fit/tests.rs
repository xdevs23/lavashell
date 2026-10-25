// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::geometry::{PixelSize, Point};
use crate::units::{DpExt, PctExt};

const VIEWPORT: Size = Size::new(Dp::new(400.0), Dp::new(200.0));
const SQUARE: Size = Size::new(Dp::new(100.0), Dp::new(100.0));
const WIDE: Size = Size::new(Dp::new(100.0), Dp::new(50.0));

#[test]
fn cover_overflows_the_sides_at_each_anchor() {
    // A 2:1 image covers the square box once its height matches the box. The anchor then moves the image sideways.
    let expected = [
        (Anchor::TopLeft, 0),
        (Anchor::Top, -50),
        (Anchor::TopRight, -100),
        (Anchor::Left, 0),
        (Anchor::Center, -50),
        (Anchor::Right, -100),
        (Anchor::BottomLeft, 0),
        (Anchor::Bottom, -50),
        (Anchor::BottomRight, -100),
    ];
    for (anchor, x) in expected {
        let fit = ImageFit::Cover(ImagePlacement::at(anchor));
        assert_eq!(
            fit.resolve(PixelSize::new(200, 100), SQUARE, VIEWPORT),
            Rect::new(x.dp(), 0.dp(), 200.dp(), 100.dp()),
            "{anchor:?}"
        );
    }
}

#[test]
fn contain_leaves_the_box_empty_above_and_below_at_each_anchor() {
    // A 2:1 image fits into the square box once its width matches the box. The box then has empty space above and
    // below the image.
    let expected = [
        (Anchor::TopLeft, 0),
        (Anchor::Top, 0),
        (Anchor::TopRight, 0),
        (Anchor::Left, 25),
        (Anchor::Center, 25),
        (Anchor::Right, 25),
        (Anchor::BottomLeft, 50),
        (Anchor::Bottom, 50),
        (Anchor::BottomRight, 50),
    ];
    for (anchor, y) in expected {
        let fit = ImageFit::Contain(ImagePlacement::at(anchor));
        assert_eq!(
            fit.resolve(PixelSize::new(200, 100), SQUARE, VIEWPORT),
            Rect::new(0.dp(), y.dp(), 100.dp(), 50.dp()),
            "{anchor:?}"
        );
    }
}

#[test]
fn stretch_and_an_image_with_the_ratio_of_the_box_fill_the_box() {
    let square = Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp());
    assert_eq!(
        ImageFit::Stretch.resolve(PixelSize::new(300, 100), SQUARE, VIEWPORT),
        square
    );
    let wide = Rect::new(0.dp(), 0.dp(), 100.dp(), 50.dp());
    for fit in [ImageFit::cover(), ImageFit::contain(), ImageFit::Stretch] {
        assert_eq!(
            fit.resolve(PixelSize::new(400, 200), WIDE, VIEWPORT),
            wide,
            "{fit:?}"
        );
    }
}

#[test]
fn an_offset_moves_the_image_along_the_width_and_the_height_of_the_box() {
    let moved = |x: Length, y: Length| {
        let frame = ImageFit::Cover(ImagePlacement::default().offset(x, y)).resolve(
            PixelSize::new(200, 100),
            WIDE,
            VIEWPORT,
        );
        Point::new(frame.x, frame.y)
    };
    assert_eq!(
        moved(5.dp().into(), (-3).dp().into()),
        Point::new(5.dp(), (-3).dp())
    );
    assert_eq!(
        moved(50.pct().into(), 50.pct().into()),
        Point::new(50.dp(), 25.dp())
    );
}

#[test]
fn a_frame_is_measured_along_the_width_and_the_height_of_the_box() {
    let zoomed = ImageFrame::new(-50.pct(), -50.pct(), 200.pct(), 200.pct());
    assert_eq!(
        ImageFit::Frame(zoomed).resolve(PixelSize::new(400, 200), WIDE, VIEWPORT),
        Rect::new((-50).dp(), (-25).dp(), 200.dp(), 100.dp())
    );
}
