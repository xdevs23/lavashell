// SPDX-License-Identifier: GPL-3.0-only
use super::outline::ContourPath;
use super::program::{LENS_SHARE, SHORTEST_LENS};
use super::test_shapes::{Distance, ellipse, star, union};
use super::*;
use crate::color::HALF_EPSILON;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_core::shape::PointsError;
use skia_safe::{AlphaType, ColorType, ImageInfo, Paint, Shader, surfaces};

mod holes;
mod programs;

const SIZE: (u16, u16) = (200, 120);

fn dimensions(scale: i32) -> (i32, i32) {
    (i32::from(SIZE.0) * scale, i32::from(SIZE.1) * scale)
}

fn pixels(shader: Shader) -> Result<Vec<[f64; 4]>, TestError> {
    let info = ImageInfo::new(dimensions(1), ColorType::RGBAF32, AlphaType::Premul, None);
    let mut surface = surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface
        .canvas()
        .draw_paint(Paint::default().set_shader(shader));
    let mut bytes = vec![0u8; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)));
    let (floats, _) = bytes.as_chunks::<4>();
    let (pixels, _) = floats.as_chunks::<4>();
    Ok(pixels
        .iter()
        .map(|pixel| pixel.map(|float| f64::from(f32::from_ne_bytes(float))))
        .collect())
}

fn points() -> impl Iterator<Item = (usize, Point)> {
    (0..SIZE.1).flat_map(|y| {
        (0..SIZE.0).map(move |x| {
            let index = usize::from(y) * usize::from(SIZE.0) + usize::from(x);
            (
                index,
                Point::new(Dp::new(f64::from(x) + 0.5), Dp::new(f64::from(y) + 0.5)),
            )
        })
    })
}

fn analytic(main: &'static str, reads: Content, shape: &Shape) -> Result<Vec<[f64; 4]>, TestError> {
    let room = FieldRoom::new(UniformRange::VULKAN_REQUIRED);
    let fields = Fields::new(&room)?;
    let programs = Analytic::<()>::new("test", reads, &[Part::new(main)], Pass::Own(&room))?;
    let bound = programs
        .of(&fields, shape)?
        .ok_or(TestError::Missing("analytic field"))?;
    pixels(bound.evaluate(&(), ())?)
}

const EXACT: &str = "half4 main(float2 p) { return half4(exactField(p).z, 0.0, 0.0, 1.0); }";

fn exact(shape: &Shape) -> Result<Vec<f64>, TestError> {
    let pixels = analytic(EXACT, Content::Distance, shape)?;
    Ok(pixels.into_iter().map(|pixel| pixel[0]).collect())
}

fn pill(x: f64, width: f64) -> Shape {
    RoundedRect::new(
        Rect::new(Dp::new(x), 30.dp(), Dp::new(width), 60.dp()),
        Corners::all(30.dp()),
    )
    .into()
}

fn unions() -> Result<Vec<Shape>, TestError> {
    let boxed = RoundedRect::new(
        Rect::new(10.dp(), 20.dp(), 120.dp(), 70.dp()),
        Corners::new(30.dp(), 0.dp(), 12.dp(), 45.dp()),
    );
    Ok(vec![
        boxed.into(),
        union([pill(10.0, 80.0), pill(104.0, 80.0)], 30.dp())?,
        union(
            [pill(10.0, 80.0), pill(90.0, 60.0), pill(140.0, 50.0)],
            24.dp(),
        )?,
        union(
            [
                pill(20.0, 60.0),
                RoundedRect::new(
                    Rect::new(70.dp(), 10.dp(), 50.dp(), 40.dp()),
                    Corners::new(0.dp(), 20.dp(), 4.dp(), 8.dp()),
                )
                .into(),
            ],
            0.dp(),
        )?,
    ])
}

#[test]
fn union_fields_match_shape_distance() -> Result<(), TestError> {
    for shape in unions()? {
        let field = exact(&shape)?;
        for (index, p) in points() {
            let expected = shape.distance(p);
            let tolerance = 1e-3 + expected.abs() * 1e-5;
            assert!(
                (field[index] - expected).abs() <= tolerance,
                "{shape:?} at {p:?}: shader {} and CPU {expected}",
                field[index]
            );
        }
    }
    Ok(())
}

/// The most points that a star can have while `programs` still evaluates its contour without a field texture.
fn most_star_points(fields: &Fields, programs: &Analytic<()>) -> Result<u16, TestError> {
    let program =
        |points| -> Result<Option<_>, TestError> { Ok(programs.of(fields, &star(points, 26.0)?)?) };
    let (mut fitting, mut exceeding) = (10, 20);
    while program(exceeding)?.is_some() {
        (fitting, exceeding) = (exceeding, 2 * exceeding);
    }
    while exceeding - fitting > 1 {
        let middle = fitting + (exceeding - fitting) / 2;
        if program(middle)?.is_some() {
            fitting = middle;
        } else {
            exceeding = middle;
        }
    }
    Ok(fitting)
}

#[test]
fn contour_fields_match_shape_distance() -> Result<(), TestError> {
    let room = FieldRoom::new(UniformRange::VULKAN_REQUIRED);
    let fields = Fields::new(&room)?;
    let programs = Analytic::<()>::new(
        "test",
        Content::Distance,
        &[Part::new(EXACT)],
        Pass::Own(&room),
    )?;
    let most = most_star_points(&fields, &programs)?;
    assert!(programs.of(&fields, &star(most + 1, 26.0)?)?.is_none());
    for shape in [star(10, 26.0)?, star(most, 26.0)?] {
        let field = exact(&shape)?;
        for (index, p) in points() {
            let expected = shape.distance(p);
            assert!(
                (field[index] - expected).abs() <= 1e-3 + expected.abs() * 1e-5,
                "at {p:?}: shader {} and CPU {expected}",
                field[index]
            );
        }
    }
    Ok(())
}

fn pentagram() -> Result<Shape, PointsError> {
    let points = (0..5_u8).map(|i| {
        let angle = std::f64::consts::TAU * f64::from(2 * i) / 5.0;
        Point::new(
            Dp::new(100.0 + 50.0 * angle.sin()),
            Dp::new(60.0 - 50.0 * angle.cos()),
        )
    });
    Ok(Shape::Contour(Polygon::new(points.collect::<Vec<_>>())?))
}

fn contour(points: &[(f64, f64)]) -> Result<Shape, PointsError> {
    let points = points
        .iter()
        .map(|&(x, y)| Point::new(Dp::new(x), Dp::new(y)));
    Ok(Shape::Contour(Polygon::new(points.collect::<Vec<_>>())?))
}

#[test]
fn outline_segments_have_the_filled_side_to_their_right() -> Result<(), TestError> {
    let reversed = |shape: Shape| -> Result<Shape, TestError> {
        let Shape::Contour(polygon) = shape else {
            return Err(TestError::Unexpected("a contour"));
        };
        let points = polygon.points().iter().rev().copied().collect::<Vec<_>>();
        Ok(Shape::Contour(Polygon::new(points)?))
    };
    let heptagram = (0..7_u8).map(|i| {
        let angle = std::f64::consts::TAU * f64::from(3 * i) / 7.0;
        (100.0 + 80.0 * angle.sin(), 60.0 - 50.0 * angle.cos())
    });
    let cases = [
        star(12, 26.0)?,
        reversed(star(12, 26.0)?)?,
        pentagram()?,
        contour(&heptagram.collect::<Vec<_>>())?,
        contour(&[
            (20.0, 20.0),
            (180.0, 20.0),
            (180.0, 90.0),
            (100.0, 90.0),
            (100.0, 115.0),
            (100.0, 90.0),
            (20.0, 90.0),
        ])?,
        contour(&[
            (20.0, 20.0),
            (180.0, 20.0),
            (180.0, 90.0),
            (60.0, 90.0),
            (140.0, 90.0),
            (20.0, 90.0),
        ])?,
    ];
    for shape in cases {
        let Shape::Contour(polygon) = &shape else {
            return Err(TestError::Unexpected("a contour"));
        };
        let outline = ContourPath::new(polygon);
        assert!(!outline.segments().is_empty(), "{shape:?}");
        for segment in outline.segments().chunks(SEGMENT_FLOATS) {
            let (a, b) = (
                Point::new(Dp::new(segment[0]), Dp::new(segment[1])),
                Point::new(Dp::new(segment[2]), Dp::new(segment[3])),
            );
            let length = (b.x - a.x).hypot(b.y - a.y);
            let right = (-(b.y - a.y) / length, (b.x - a.x) / length);
            let middle = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let beside = |side: f64| {
                shape.distance(middle.offset(Dp::new(right.0 * side), Dp::new(right.1 * side)))
            };
            assert!(
                beside(0.5) < 0.0 && beside(-0.5) > 0.0,
                "{shape:?}: {segment:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_polygon_without_area_has_no_outline() -> Result<(), TestError> {
    let Shape::Contour(polygon) = contour(&[(20.0, 20.0), (120.0, 20.0), (60.0, 20.0)])? else {
        return Err(TestError::Unexpected("a contour"));
    };
    assert_eq!(ContourPath::new(&polygon).segments(), []);
    Ok(())
}

fn steepest(shape: &Shape, change: impl Fn(usize, usize) -> f64) -> f64 {
    let (columns, rows) = (usize::from(SIZE.0), usize::from(SIZE.1));
    let centers = points().map(|(_, p)| p).collect::<Vec<_>>();
    let inside = |index: usize| shape.distance(centers[index]) < 0.0;
    (0..columns * rows)
        .filter(|&index| inside(index))
        .flat_map(|index| {
            let right = (index % columns + 1 < columns).then_some(index + 1);
            let below = (index / columns + 1 < rows).then_some(index + columns);
            [right, below]
                .into_iter()
                .flatten()
                .filter(|&next| inside(next))
                .map(move |next| (index, next))
        })
        .map(|(index, next)| change(index, next))
        .fold(0.0, f64::max)
}

fn steepest_turn(shape: &Shape, lens: &[[f64; 4]]) -> f64 {
    steepest(shape, |a, b| {
        let (n, m) = (lens[a], lens[b]);
        (n[0] - m[0]).hypot(n[1] - m[1]) * n[3].max(m[3])
    })
}

fn crease_cases() -> Result<Vec<Shape>, TestError> {
    let boxed = |x, y, width, height| {
        Shape::from(RoundedRect::new(
            Rect::new(x, y, width, height),
            Corners::all(12.dp()),
        ))
    };
    Ok(vec![
        star(12, 26.0)?,
        union(
            [
                boxed(20.dp(), 20.dp(), 160.dp(), 40.dp()),
                boxed(20.dp(), 20.dp(), 50.dp(), 90.dp()),
            ],
            0.dp(),
        )?,
        union([pill(10.0, 80.0), pill(84.0, 80.0)], 4.dp())?,
    ])
}

fn analytic_lens(shape: &Shape) -> Result<Vec<[f64; 4]>, TestError> {
    analytic(
        "half4 main(float2 p) { float exact; return half4(lensField(p, exact)); }",
        Content::Lens,
        shape,
    )
}

fn texture_lens(shape: &Shape) -> Result<Vec<[f64; 4]>, TestError> {
    let mut surface = surfaces::raster_n32_premul(dimensions(4)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas();
    let _chained_canvas = canvas.scale((4.0, 4.0));
    let texels = TexelPixels::lens(canvas).ok_or(TestError::Missing("texel size of the lens"))?;
    let lens = Fields::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?
        .lens(canvas, &Solid::of(shape), texels)?
        .ok_or(TestError::Missing("lens texture"))?;
    pixels(lens.shader()?)
}

/// A crease in the lens turns the lens normal within a few dp. A smooth lens
/// spreads a reversal of the normal, a change of 2, over at least one lens
/// length. The lens blends with polynomial steps, which change at most 1.5
/// times as fast as their mean rate, like smoothstep in its middle.
#[test]
fn lens_normals_turn_no_faster_than_their_lens_length() -> Result<(), TestError> {
    let nested = union(
        [
            star(12, 30.0)?,
            union([pill(130.0, 60.0), pill(150.0, 40.0)], 8.dp())?,
        ],
        2.dp(),
    )?;
    let mut turns = Vec::new();
    for shape in crease_cases()? {
        turns.push(steepest_turn(&shape, &analytic_lens(&shape)?));
    }
    for shape in [star(12, 26.0)?, star(40, 30.0)?, nested] {
        turns.push(steepest_turn(&shape, &texture_lens(&shape)?));
    }
    let steepest = 2.0 * 1.5;
    assert!(
        turns.iter().all(|&turn| turn <= steepest),
        "turns of the lens normal per lens length: {turns:?}"
    );
    Ok(())
}

fn neighbours(index: usize) -> impl Iterator<Item = usize> {
    let (columns, rows) = (usize::from(SIZE.0), usize::from(SIZE.1));
    let (x, y) = (index % columns, index / columns);
    [
        (x > 0).then(|| index - 1),
        (x + 1 < columns).then(|| index + 1),
        (y > 0).then(|| index - columns),
        (y + 1 < rows).then(|| index + columns),
    ]
    .into_iter()
    .flatten()
}

/// Checks `found` against `expected` at every pixel inside `shape`. A field
/// texture interpolates between texel centres 1 dp apart. A value may therefore
/// differ by the change of the lens toward the neighbouring pixels along x and
/// along y. The half floats of the texture round both the value and the distance
/// it was built from, each by half a step at their size, or at 1 below 1.
fn assert_matches(shape: &Shape, expected: &[[f64; 4]], found: &[[f64; 4]]) {
    for (index, p) in points().filter(|(_, p)| shape.distance(*p) < 0.0) {
        for channel in 0..4 {
            let a = expected[index][channel];
            let step = neighbours(index)
                .map(|next| (expected[next][channel] - a).abs())
                .fold(0.0, f64::max);
            let b = found[index][channel];
            assert!(
                (a - b).abs() <= 2.0 * step + (a.abs() + 1.0) * HALF_EPSILON,
                "at {p:?}: channel {channel} is {a} and {b} from the texture"
            );
        }
    }
}

#[test]
fn lens_textures_match_the_analytic_lens() -> Result<(), TestError> {
    for shape in crease_cases()? {
        assert_matches(&shape, &analytic_lens(&shape)?, &texture_lens(&shape)?);
    }
    Ok(())
}

fn reference_lens(polygon: &Polygon) -> Vec<[f64; 4]> {
    let bounds = polygon.bounds();
    let length = ((bounds.width.min(bounds.height) / 2.0).get() * LENS_SHARE).max(SHORTEST_LENS);
    let shape = Shape::Contour(polygon.clone());
    // Each segment as its start, its edge, the squared length of the edge and its outward normal.
    let segments = ContourPath::new(polygon)
        .segments()
        .chunks(SEGMENT_FLOATS)
        .map(|s| {
            let (edge_x, edge_y) = (s[2] - s[0], s[3] - s[1]);
            let along = edge_x.hypot(edge_y);
            let squared = along * along;
            [
                s[0],
                s[1],
                edge_x,
                edge_y,
                squared,
                edge_y / along,
                -edge_x / along,
            ]
        })
        .collect::<Vec<_>>();
    points()
        .map(|(_, p)| {
            let exact = shape.distance(p);
            let mut sum = [0.0f64; 4];
            for &[x, y, edge_x, edge_y, squared, out_x, out_y] in &segments {
                let (to_x, to_y) = (p.x.get() - x, p.y.get() - y);
                let t = ((to_x * edge_x + to_y * edge_y) / squared).clamp(0.0, 1.0);
                let distance = (to_x - edge_x * t).hypot(to_y - edge_y * t);
                let weight = (1.0 - (distance - exact.abs()) / length).max(0.0);
                let weight = weight * weight;
                let line = to_x * out_x + to_y * out_y;
                sum[0] += out_x * weight;
                sum[1] += out_y * weight;
                sum[2] += (line.min(exact) - exact) * weight;
                sum[3] += weight;
            }
            [
                sum[0] / sum[3],
                sum[1] / sum[3],
                sum[2] / sum[3] + exact,
                length,
            ]
        })
        .collect()
}

/// Deep inside a large circle, the lens sums over all chunks together exceed the range of half floats. The lens drawn
/// chunk by chunk still matches the lens computed on the CPU.
#[test]
fn the_chunked_lens_of_a_large_contour_matches_the_cpu() -> Result<(), TestError> {
    let circle = ellipse(
        1024,
        Rect::new(-1900.dp(), -2940.dp(), 4000.dp(), 4000.dp()),
    )?;
    let Shape::Contour(polygon) = &circle else {
        return Err(TestError::Unexpected("a contour"));
    };
    assert_matches(&circle, &reference_lens(polygon), &texture_lens(&circle)?);
    Ok(())
}

/// A smooth maximum over one lens length lifts the lens distance by at most a
/// quarter of that length, so half a lens length inside the outline the lens
/// distance is negative and the lens bends light outward.
#[test]
fn lens_distances_are_negative_half_a_lens_length_inside() -> Result<(), TestError> {
    let cases = unions()?.into_iter().chain(crease_cases()?).chain([
        ellipse(16, Rect::new(20.dp(), 15.dp(), 160.dp(), 90.dp()))?,
        pentagram()?,
    ]);
    for shape in cases {
        let lens = analytic_lens(&shape)?;
        for (index, p) in points() {
            let [.., distance, length] = lens[index];
            assert!(
                shape.distance(p) > -length / 2.0 || distance < 0.0,
                "{shape:?} at {p:?}: lens distance {distance}"
            );
        }
    }
    Ok(())
}
