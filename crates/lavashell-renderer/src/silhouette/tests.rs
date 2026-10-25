// SPDX-License-Identifier: GPL-3.0-only
use super::BandEdges;
use crate::effects_tests::{draw, draw_with, sample, stripes, surface, tangle};
use crate::field::Band;
use crate::field::Route;
use crate::field::test_shapes::{star, union};
use crate::float_asserts::same_at;
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::test_surfaces::ChannelRounding;
use crate::{FieldRoom, UniformRange};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;

const RED: Color = Color::rgb(0x00ff_0000);

fn square() -> Result<Shape, TestError> {
    Ok(Shape::Contour(Polygon::new([
        Point::new(20.dp(), 10.dp()),
        Point::new(50.dp(), 10.dp()),
        Point::new(50.dp(), 40.dp()),
        Point::new(20.dp(), 40.dp()),
    ])?))
}

fn red(color: [f32; 4]) -> bool {
    color[3] > 0.98 && color[0] > 0.98 && color[1] < 0.01
}

#[test]
fn four_radii_fill_round_only_their_own_corners() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let shape = RoundedRect::new(
        Rect::new(8.dp(), 8.dp(), 80.dp(), 32.dp()),
        Corners::new(16.dp(), 0.dp(), 16.dp(), 0.dp()),
    );
    draw(&mut s, |d| d.material(shape, &Material::new(Color::WHITE)))?;
    assert!(sample(&mut s, 9, 9)[3] < 0.01);
    assert!(sample(&mut s, 86, 9)[3] > 0.98);
    assert!(sample(&mut s, 86, 38)[3] < 0.01);
    assert!(sample(&mut s, 9, 38)[3] > 0.98);
    Ok(())
}

/// An inset shadow moved far off its shape covers the whole inside of the shape. The render then shows the edge of
/// the clip by the shape, which matches the edge of the filled shape.
#[test]
fn rounded_boxes_fill_and_clip_along_one_edge() -> Result<(), TestError> {
    let clip_edge = Shadow::new(Color::WHITE, 0.dp(), Point::new(200.dp(), 0.dp()))?.inset(true);
    for corners in [
        Corners::all(9.dp()),
        Corners::new(16.dp(), 3.dp(), 11.dp(), 0.dp()),
    ] {
        let shape = RoundedRect::new(Rect::new(8.3.dp(), 6.6.dp(), 78.dp(), 34.dp()), corners);
        let filled = render(false, |d| d.material(shape, &Material::new(Color::WHITE)))?;
        let clipped = render(false, |d| {
            d.material(shape, &Material::new(Color::TRANSPARENT).shadow(clip_edge));
        })?;
        let worst = f64::from(difference(&filled, &clipped));
        // Each of the two renders rounds its white pixels once.
        assert!(
            worst <= ChannelRounding::of(&surface()?)?.after(2, 1.0),
            "{corners:?}: largest channel difference {worst}"
        );
    }
    Ok(())
}

#[test]
fn contour_borders_follow_their_placement() -> Result<(), TestError> {
    for (placement, inside, outside) in [
        (BorderPlacement::Inside, true, false),
        (BorderPlacement::Center, true, true),
        (BorderPlacement::Outside, false, true),
    ] {
        let mut s = surface()?;
        let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
        let material =
            Material::new(Color::WHITE).border(Stroke::new(RED, 4.dp())?.placement(placement));
        let contour = square()?;
        draw(&mut s, |d| d.material(contour, &material))?;
        assert_eq!(red(sample(&mut s, 21, 25)), inside, "{placement:?}");
        assert_eq!(red(sample(&mut s, 18, 25)), outside, "{placement:?}");
        assert!(!red(sample(&mut s, 35, 25)));
        assert!(sample(&mut s, 35, 25)[3] > 0.98);
    }
    Ok(())
}

#[test]
fn contour_shadows_spread_and_inset_around_the_outline() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let material = Material::new(Color::TRANSPARENT)
        .shadow(Shadow::new(RED, 0.dp(), Point::new(0.dp(), 0.dp()))?.spread(4.dp())?)
        .shadow(
            Shadow::new(Color::WHITE, 0.dp(), Point::new(0.dp(), 0.dp()))?
                .spread(3.dp())?
                .inset(true),
        );
    let contour = square()?;
    draw(&mut s, |d| d.material(contour, &material))?;
    assert!(red(sample(&mut s, 17, 25)));
    assert!(sample(&mut s, 14, 25)[3] < 0.01);
    assert!(sample(&mut s, 21, 25)[3] > 0.98 && sample(&mut s, 21, 25)[1] > 0.98);
    assert!(sample(&mut s, 35, 25)[3] < 0.01);
    Ok(())
}

#[test]
fn unions_fill_every_member() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let fused = union(
        [
            Shape::rounded(Rect::new(4.dp(), 4.dp(), 30.dp(), 40.dp()), 8.dp()),
            square()?.translated(Point::new(40.dp(), 0.dp())),
        ],
        0.dp(),
    )?;
    draw(&mut s, |d| d.material(fused, &Material::new(Color::WHITE)))?;
    assert!(sample(&mut s, 18, 24)[3] > 0.98);
    assert!(sample(&mut s, 75, 24)[3] > 0.98);
    assert!(sample(&mut s, 50, 24)[3] < 0.01);
    Ok(())
}

#[test]
fn backdrops_on_other_shapes_blur_inside_the_shape() -> Result<(), TestError> {
    let mut s = surface()?;
    let glass = Backdrop::glass(
        5.dp(),
        Glass {
            refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
            dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
            rim: constant!(NonNegative::try_new(0.14)),
        },
    )?;
    draw(&mut s, |d| {
        stripes(d);
        let shape = RoundedRect::new(
            Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()),
            Corners::new(20.dp(), 0.dp(), 0.dp(), 0.dp()),
        );
        d.material(shape, &Material::new(Color::TRANSPARENT).backdrop(glass));
    })?;
    // The cut-off top-left corner has a sharp stripe.
    assert!(sample(&mut s, 17, 5)[0] > 0.98);
    for x in [35, 77] {
        let blurred = sample(&mut s, x, 24)[0];
        assert!((blurred - 0.5).abs() < 0.025, "{x}: {blurred}");
    }
    assert!((sample(&mut s, 78, 5)[0] - 0.5).abs() < 0.025);
    Ok(())
}

fn pills(gap: f64, spacing: f64) -> Result<Shape, TestError> {
    let pill = |x: f64| Shape::rounded(Rect::new(Dp::new(x), 14.dp(), 30.dp(), 20.dp()), 10.dp());
    union(
        [pill(48.0 - gap / 2.0 - 30.0), pill(48.0 + gap / 2.0)],
        Dp::new(spacing),
    )
}

#[test]
fn unions_fuse_once_their_gap_reaches_the_spacing() -> Result<(), TestError> {
    for (gap, fused) in [(16.0, false), (10.0, true), (6.0, true), (0.0, true)] {
        let mut s = surface()?;
        let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
        let pair = pills(gap, 12.0)?;
        draw(&mut s, |d| {
            d.material(pair, &Material::new(Color::WHITE));
        })?;
        let neck = sample(&mut s, 48, 24)[3];
        assert_eq!(neck > 0.5, fused, "gap {gap}: {neck}");
        assert!(sample(&mut s, 20, 24)[3] > 0.98);
    }
    Ok(())
}

#[test]
fn fused_unions_keep_aligned_straight_edges_straight() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let fused = union(
        [
            Shape::rounded(Rect::new(10.dp(), 14.dp(), 34.dp(), 20.dp()), 0.dp()),
            Shape::rounded(Rect::new(48.dp(), 14.dp(), 34.dp(), 20.dp()), 0.dp()),
        ],
        16.dp(),
    )?;
    draw(&mut s, |d| d.material(fused, &Material::new(Color::WHITE)))?;
    assert!(sample(&mut s, 46, 24)[3] > 0.98);
    assert!(sample(&mut s, 46, 12)[3] < 0.01);
    assert!(sample(&mut s, 46, 35)[3] < 0.01);
    Ok(())
}

#[test]
fn union_borders_and_shadows_follow_the_fused_outline() -> Result<(), TestError> {
    let fused = pills(4.0, 12.0)?;
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let material = Material::new(Color::TRANSPARENT).border(Stroke::new(RED, 3.dp())?);
    draw(&mut s, |d| d.material(fused.clone(), &material))?;
    assert!(red(sample(&mut s, 17, 24)));
    assert!(sample(&mut s, 13, 24)[3] < 0.01);
    assert!(
        sample(&mut s, 48, 24)[3] < 0.01,
        "the neck has no border inside"
    );

    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let material = Material::new(Color::TRANSPARENT)
        .shadow(Shadow::new(RED, 0.dp(), Point::new(0.dp(), 0.dp()))?.spread(4.dp())?)
        .shadow(
            Shadow::new(Color::WHITE, 0.dp(), Point::new(0.dp(), 0.dp()))?
                .spread(3.dp())?
                .inset(true),
        );
    draw(&mut s, |d| d.material(fused, &material))?;
    assert!(red(sample(&mut s, 14, 24)));
    assert!(sample(&mut s, 10, 24)[3] < 0.01);
    assert!(sample(&mut s, 17, 24)[3] > 0.98 && sample(&mut s, 17, 24)[1] > 0.98);
    assert!(
        sample(&mut s, 48, 24)[3] < 0.01,
        "the neck is deeper than the inset"
    );
    Ok(())
}

#[test]
fn unions_with_contour_members_cover_through_a_field_texture() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let fused = union(
        [
            Shape::rounded(Rect::new(4.dp(), 8.dp(), 26.dp(), 32.dp()), 8.dp()),
            Shape::Contour(Polygon::new([
                Point::new(36.dp(), 40.dp()),
                Point::new(48.dp(), 8.dp()),
                Point::new(60.dp(), 40.dp()),
            ])?),
            Shape::rounded(Rect::new(66.dp(), 8.dp(), 26.dp(), 32.dp()), 8.dp()),
        ],
        0.dp(),
    )?;
    draw(&mut s, |d| d.material(fused, &Material::new(Color::WHITE)))?;
    for x in [17, 48, 79] {
        assert!(sample(&mut s, x, 30)[3] > 0.98, "member at {x}");
    }
    for x in [33, 63] {
        assert!(sample(&mut s, x, 30)[3] < 0.02, "gap at {x}");
    }
    Ok(())
}

#[test]
fn contour_outlines_from_raster_tiles_match_skia_paths() -> Result<(), TestError> {
    let diamond = Shape::Contour(Polygon::new([
        Point::new(35.dp(), 6.dp()),
        Point::new(60.dp(), 24.dp()),
        Point::new(35.dp(), 42.dp()),
        Point::new(10.dp(), 24.dp()),
    ])?);
    let material = Material::new(Color::WHITE.with_alpha(0.3))
        .backdrop(Backdrop::glass(
            3.dp(),
            Glass {
                refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
                dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                rim: constant!(NonNegative::try_new(0.14)),
            },
        )?)
        .border(Stroke::new(RED, 2.dp())?.placement(BorderPlacement::Outside))
        .shadow(
            Shadow::new(Color::rgb(0x0000_00ff), 3.dp(), Point::new(2.dp(), 1.dp()))?
                .spread(2.dp())?,
        )
        .shadow(
            Shadow::new(Color::rgb(0x0000_ff00), 2.dp(), Point::new(1.dp(), 2.dp()))?.inset(true),
        );
    let paint = |d: &mut Drawing<'_>| {
        stripes(d);
        d.material(diamond.clone(), &material);
    };
    let worst = difference(&render(false, paint)?, &render(true, paint)?);
    assert!(worst < 0.01, "largest channel difference {worst}");
    Ok(())
}

/// Raster tiles draw the blur inside the clip in two cases: when the band lies just outside the clip, and when the
/// band is larger than the tile cache, so that only a cropped part of the band gets tiles.
#[test]
fn blurred_contour_shadows_from_raster_tiles_reach_into_the_clip() -> Result<(), TestError> {
    let contour = square()?;
    let shadow = Material::new(Color::TRANSPARENT).shadow(Shadow::new(
        RED,
        4.dp(),
        Point::new(0.dp(), 0.dp()),
    )?);
    let beside = |d: &mut Drawing<'_>| {
        d.clip(
            Rect::new(0.dp(), 0.dp(), 48.dp(), 48.dp()),
            Corners::default(),
        );
        d.material(
            contour.clone().translated(Point::new(32.dp(), 0.dp())),
            &shadow,
        );
    };
    // The left edge of the square lies at x = 48 px and its band spans 2400 px.
    let enlarged = |d: &mut Drawing<'_>| {
        d.transform(Affine::scale(80.0, 80.0).translated(Point::new(-1552.dp(), -1000.dp())));
        d.material(contour.clone(), &shadow);
    };
    let cases: [&dyn Fn(&mut Drawing<'_>); 2] = [&beside, &enlarged];
    for paint in cases {
        let skia = render(false, paint)?;
        assert!(
            skia[25 * 96 + 46][3] > 0.02,
            "the blur reaches into the clip"
        );
        let worst = difference(&skia, &render(true, paint)?);
        assert!(worst < 0.01, "largest channel difference {worst}");
    }
    Ok(())
}

fn render(raster: bool, paint: impl FnOnce(&mut Drawing<'_>)) -> Result<Vec<[f32; 4]>, TestError> {
    let mut s = surface()?;
    let painter = crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    draw_with(painter.with_raster_paths(raster), &mut s, paint)?;
    Ok((0..48)
        .flat_map(|y| (0..96).map(move |x| (x, y)))
        .map(|(x, y)| sample(&mut s, x, y))
        .collect())
}

fn difference(a: &[[f32; 4]], b: &[[f32; 4]]) -> f32 {
    let premultiplied = |c: &[f32; 4]| [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
    a.iter()
        .zip(b)
        .flat_map(|(a, b)| {
            let (a, b) = (premultiplied(a), premultiplied(b));
            std::array::from_fn::<f32, 4, _>(|i| (a[i] - b[i]).abs())
        })
        .fold(0.0, f32::max)
}

/// Frames of another surface between two frames that draw the contour do not drop the path and its band regions. A
/// frame of the same surface that does not draw the contour drops them.
#[test]
fn a_contour_keeps_its_path_and_band_regions_while_frames_draw_it() -> Result<(), TestError> {
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((200, 120)).ok_or(TestError::RasterSurface)?;
    let mut resources = PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let shape = star(12, 26.0)?;
    let band = Band::stroke(Stroke::new(Color::WHITE, 4.dp())?);
    let mut made = 0;
    let mut draw =
        |resources: &PaintResources, canvas: &skia_safe::Canvas| -> Result<_, TestError> {
            let Route::Path(outline) = resources.fields.route(&shape)? else {
                return Err(TestError::Missing("path of a contour"));
            };
            let edges = BandEdges::new(canvas, &outline.path, band)?;
            let region = outline.region(resources.fields.frames(), edges.numbers(), |path| {
                made += 1;
                edges.path(path)
            });
            assert!(region.is_some());
            Ok(outline)
        };
    resources.begin_frame(SurfaceId(1));
    let first = draw(&resources, surface.canvas())?;
    resources.begin_frame(SurfaceId(2));
    resources.begin_frame(SurfaceId(1));
    let again = draw(&resources, surface.canvas())?;
    assert!(
        std::rc::Rc::ptr_eq(&first, &again),
        "one path for the frames"
    );
    resources.begin_frame(SurfaceId(1));
    resources.begin_frame(SurfaceId(1));
    let later = draw(&resources, surface.canvas())?;
    assert!(!std::rc::Rc::ptr_eq(&first, &later), "the path went unused");
    assert_eq!(made, 2, "one region for each path");
    Ok(())
}

/// The border drawn from the field paints at the outline of the circle and nowhere farther out than its width.
#[test]
fn a_border_that_path_operations_give_up_on_draws_from_the_field() -> Result<(), TestError> {
    let shape = tangle()?;
    let Shape::Contour(polygon) = &shape else {
        return Err(TestError::Unexpected("a contour"));
    };
    let mut surface = surface()?;
    let stroke = Stroke::new(Color::WHITE, 0.125.dp())?.placement(BorderPlacement::Outside);
    let path = crate::paint::skpath(polygon)?;
    let edges = BandEdges::new(surface.canvas(), &path, Band::stroke(stroke))?;
    assert!(
        edges.path(&path).is_none(),
        "the test needs a band that the path operations give up on"
    );
    let mut commands = vec![DrawCommand::Transform(
        Affine::scale(16.0, 16.0).translated(Point::new(48.dp(), 24.dp())),
    )];
    commands.push(DrawCommand::Stroke { shape, stroke });
    let mut painter =
        crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
    let mut rim: u16 = 0;
    for (x, y) in (0..48_i16).flat_map(|y| (0..96_i16).map(move |x| (x, y))) {
        let radius = (f64::from(x) + 0.5 - 48.0).hypot(f64::from(y) + 0.5 - 24.0);
        let alpha = sample(&mut surface, i32::from(x), i32::from(y))[3];
        if radius > 19.0 {
            assert!(same_at(alpha, 0.0, 0.0), "the border reaches {x}, {y}");
        } else if radius > 16.5 && alpha > 0.5 {
            rim += 1;
        }
    }
    // The border covers a ring two pixels wide around a circle of 16 pixels. At least a third of the circumference
    // of the ring is mostly covered.
    let circumference = std::f64::consts::TAU * 17.0;
    assert!(
        f64::from(rim) * 3.0 >= circumference,
        "{rim} pixels of the border outside the circle"
    );
    Ok(())
}
