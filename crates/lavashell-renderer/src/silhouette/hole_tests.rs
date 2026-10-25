// SPDX-License-Identifier: GPL-3.0-only
use crate::effects_tests::{draw, sample, surface};
use crate::failure::DrawFailure;
use crate::field::Route;
use crate::field::test_shapes::{Distance, union};
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::{FieldRoom, UniformRange};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use std::rc::Rc;

fn card() -> RoundedRect {
    RoundedRect::new(
        Rect::new(8.3.dp(), 8.6.dp(), 80.dp(), 30.5.dp()),
        Corners::all(8.dp()),
    )
}

fn lens(rect: Rect) -> RoundedRect {
    RoundedRect::new(rect, Corners::all(Dp::INFINITY))
}

fn pixel_centers() -> impl Iterator<Item = (u16, u16, Point)> {
    (0..48).flat_map(|y| {
        (0..96).map(move |x| {
            let center = Point::new(Dp::new(f64::from(x) + 0.5), Dp::new(f64::from(y) + 0.5));
            (x, y, center)
        })
    })
}

fn alpha(paint: impl FnOnce(&mut Drawing<'_>)) -> Result<Vec<f32>, TestError> {
    let mut s = surface()?;
    draw(&mut s, paint)?;
    Ok(pixel_centers()
        .map(|(x, y, _)| sample(&mut s, i32::from(x), i32::from(y))[3])
        .collect())
}

#[test]
fn a_fill_covers_the_card_and_leaves_the_hole_untouched() -> Result<(), TestError> {
    let holes = [
        lens(Rect::new(30.dp(), 14.dp(), 36.dp(), 20.dp())),
        lens(Rect::new(70.dp(), 14.dp(), 40.dp(), 20.dp())),
        lens(Rect::new(-20.dp(), -10.dp(), 50.dp(), 30.dp())),
    ];
    for hole in holes {
        let shape = card().minus(hole);
        let drawn = alpha(|d| d.fill(shape.clone(), Color::WHITE))?;
        for ((x, y, p), alpha) in pixel_centers().zip(drawn) {
            let distance = shape.distance(p);
            if distance < -1.0 {
                assert!(alpha > 0.99, "{hole:?}: {alpha} at {x}, {y}");
            } else if distance > 1.0 {
                assert!(alpha.abs() < f32::EPSILON, "{hole:?} at {x}, {y}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_hole_that_covers_the_card_leaves_nothing() -> Result<(), TestError> {
    let covering = RoundedRect::new(card().rect.outset(4.dp()), Corners::default());
    for hole in [card(), covering] {
        let drawn = alpha(|d| d.fill(card().minus(hole), Color::WHITE))?;
        assert!(drawn.iter().all(|alpha| *alpha == 0.0), "{hole:?}");
    }
    Ok(())
}

#[test]
fn a_hole_that_shares_edges_with_the_card_leaves_no_seam() -> Result<(), TestError> {
    let rect = card().rect;
    let hole = RoundedRect::new(
        Rect::new(48.dp(), rect.y, 60.dp(), rect.height),
        Corners::default(),
    );
    let drawn = alpha(|d| d.fill(card().minus(hole), Color::WHITE))?;
    for ((x, y, _), alpha) in pixel_centers().zip(drawn) {
        if (49..87).contains(&x) {
            assert!(alpha.abs() < f32::EPSILON, "seam at {x}, {y}");
        }
    }
    Ok(())
}

#[test]
fn a_hole_beside_the_card_cuts_nothing() -> Result<(), TestError> {
    let plain = alpha(|d| d.fill(card(), Color::WHITE))?;
    let beside = lens(Rect::new(90.dp(), 14.dp(), 20.dp(), 20.dp()));
    let drawn = alpha(|d| d.fill(card().minus(beside), Color::WHITE))?;
    let worst = plain
        .iter()
        .zip(&drawn)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(worst < 0.01, "{worst}");
    Ok(())
}

#[test]
fn strokes_and_shadows_follow_the_outline_of_the_hole() -> Result<(), TestError> {
    let hole = lens(Rect::new(30.dp(), 14.dp(), 36.dp(), 20.dp()));
    let shape = card().minus(hole);
    let stroke = Stroke::new(Color::WHITE, 2.dp())?.placement(BorderPlacement::Center);
    let stroke = alpha(|d| d.stroke(shape.clone(), stroke))?;
    let shadow = Shadow::new(Color::WHITE, 0.dp(), Point::new(0.dp(), 0.dp()))?.spread(3.dp())?;
    let shadow = alpha(|d| d.shadow(shape.clone(), shadow))?;
    for ((x, y, p), (stroke, shadow)) in pixel_centers().zip(stroke.into_iter().zip(shadow)) {
        let into_hole = -hole.distance(p);
        if into_hole.abs() < 0.25 {
            assert!(stroke > 0.6, "stroke {stroke} at {x}, {y}");
        }
        if into_hole > 1.0 && into_hole < 2.0 {
            assert!(shadow > 0.98, "shadow {shadow} at {x}, {y}");
        }
        if into_hole > 4.0 {
            assert!(shadow.abs() < f32::EPSILON, "shadow at {x}, {y}");
        }
        if card().distance(p) < -2.0 && into_hole < -2.0 {
            assert!(stroke.abs() < f32::EPSILON, "stroke at {x}, {y}");
            assert!(
                shadow.abs() < f32::EPSILON,
                "shadow inside the shape at {x}, {y}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_union_keeps_the_hole_of_a_member_open() -> Result<(), TestError> {
    let hollow = card().minus(lens(Rect::new(30.dp(), 14.dp(), 36.dp(), 20.dp())));
    let pill = Shape::rounded(Rect::new(90.dp(), 10.dp(), 5.dp(), 28.dp()), 2.5.dp());
    let fused = union([hollow, pill], 4.dp())?;
    let mut s = surface()?;
    draw(&mut s, |d| d.fill(fused, Color::WHITE))?;
    assert!(
        sample(&mut s, 48, 24)[3].abs() < f32::EPSILON,
        "inside the hole"
    );
    assert!(sample(&mut s, 15, 24)[3] > 0.99, "inside the card");
    assert!(sample(&mut s, 92, 24)[3] > 0.99, "inside the pill");
    Ok(())
}

/// The path made for an earlier position of the hole is dropped after a frame that does not draw it.
#[test]
fn a_moving_hole_reuses_its_path_and_drops_paths_of_earlier_positions() -> Result<(), TestError> {
    let mut resources = PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let shape = |x: f64| card().minus(lens(Rect::new(Dp::new(x), 14.dp(), 20.dp(), 20.dp())));
    let path = |resources: &PaintResources, x: f64| -> Result<_, TestError> {
        let Route::Path(outline) = resources.fields.route(&shape(x))? else {
            return Err(TestError::Missing("path of a shape with a hole"));
        };
        Ok(outline)
    };
    resources.begin_frame(SurfaceId(1));
    let first = {
        let first = path(&resources, 20.0)?;
        assert!(
            Rc::ptr_eq(&first, &path(&resources, 20.0)?),
            "one path in a frame"
        );
        Rc::downgrade(&first)
    };
    resources.begin_frame(SurfaceId(1));
    let moved = path(&resources, 30.0)?;
    assert!(
        first.upgrade().is_some(),
        "kept for the frame after its last use"
    );
    resources.begin_frame(SurfaceId(1));
    assert!(
        Rc::ptr_eq(&moved, &path(&resources, 30.0)?),
        "one path for the frames"
    );
    assert!(
        first.upgrade().is_none(),
        "dropped after a frame without it"
    );
    Ok(())
}

/// Skia draws no outline whose numbers lie beyond the range of its scalars, so routing such a shape fails.
#[test]
fn an_outline_beyond_the_range_of_skia_scalars_fails_to_route() -> Result<(), TestError> {
    let resources = PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let far = Dp::new(f64::from(f32::MAX) * 2.0);
    let hollow = card().minus(lens(Rect::new(far, 14.dp(), 20.dp(), 20.dp())));
    let contour = Shape::Contour(Polygon::new([
        Point::new(0.dp(), 0.dp()),
        Point::new(far, 0.dp()),
        Point::new(0.dp(), 10.dp()),
    ])?);
    for shape in [hollow, contour] {
        assert_eq!(
            resources.fields.route(&shape).err(),
            Some(DrawFailure::NumberRange)
        );
    }
    Ok(())
}
