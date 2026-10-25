// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::geometry::PixelSize;
use crate::image::{ImagePlacement, StreamId};
use crate::test_error::{Required, TestError, TestResult};
use crate::units::{Density, DpExt, EmExt, SpExt, VhExt};

fn screen(text_scaling: TextScaling) -> DrawContext {
    DrawContext {
        viewport: Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        text_scaling,
    }
}

fn map(affine: Affine, point: Point) -> Point {
    let [a, b, c, d] = affine.linear;
    Point::new(a * point.x + b * point.y, c * point.x + d * point.y)
        .offset(affine.translation.x, affine.translation.y)
}

#[test]
fn affines_translate_scale_and_turn_clockwise_on_screen() {
    let point = Point::new(2.dp(), 3.dp());
    let moved = map(Affine::translate(Point::new(5.dp(), -1.dp())), point);
    assert_eq!(moved, Point::new(7.dp(), 2.dp()));
    assert_eq!(
        map(Affine::scale(2.0, 0.5), point),
        Point::new(4.dp(), 1.5.dp())
    );
    let scaled_then_moved = Affine::scale(2.0, 0.5).translated(Point::new(5.dp(), -1.dp()));
    assert_eq!(map(scaled_then_moved, point), Point::new(9.dp(), 0.5.dp()));
    let turned = map(
        Affine::rotate(std::f64::consts::FRAC_PI_2),
        Point::new(1.dp(), 0.dp()),
    );
    // `FRAC_PI_2` differs from π / 2 by at most half the spacing of f64 values near π / 2, which is
    // `f64::EPSILON / 2`. The cosine of `FRAC_PI_2`, which is x of the turned point, is no larger than that difference.
    assert!(turned.x.abs() <= Dp::new(f64::EPSILON / 2.0), "{turned:?}");
    assert_f64_eq(turned.y.get(), 1.0);
}

#[test]
fn a_transformed_drawing_paints_at_its_origin_through_the_affine() {
    let mut commands = Vec::new();
    let screen = screen(TextScaling::default());
    let mut drawing = Drawing::new(
        &mut commands,
        Rect::new(10.dp(), 20.dp(), 50.dp(), 50.dp()),
        &screen,
    );
    let square = Rect::new(1.dp(), 1.dp(), 2.dp(), 2.dp());
    drawing.transformed(Affine::scale(2.0, 2.0), |drawing| {
        drawing.fill(square, Color::WHITE);
    });
    let doubled_at_origin = Affine {
        linear: [2.0, 0.0, 0.0, 2.0],
        translation: Point::new(10.dp(), 20.dp()),
    };
    assert_eq!(commands[1], DrawCommand::Transform(doubled_at_origin));
    assert_eq!(
        commands[2],
        DrawCommand::Fill {
            shape: square.into(),
            brush: Color::WHITE.into(),
        }
    );
}

#[test]
fn an_image_is_recorded_at_the_drawing_origin_with_its_frame_in_the_viewport() {
    let image = Image::encoded(Vec::new(), 640, 480);
    let mut commands = Vec::new();
    Drawing::new(
        &mut commands,
        Rect::new(10.dp(), 20.dp(), 50.dp(), 50.dp()),
        &screen(TextScaling::default()),
    )
    .image(
        &image,
        Rect::new(0.dp(), 0.dp(), 32.dp(), 24.dp()),
        ImageFit::Cover(ImagePlacement::default().offset(0.dp(), 10.vh())),
    );
    assert_eq!(
        commands,
        [DrawCommand::Image {
            image,
            destination: Rect::new(10.dp(), 20.dp(), 32.dp(), 24.dp()),
            frame: Rect::new(0.dp(), 10.dp(), 32.dp(), 24.dp()),
        }]
    );
}

#[test]
fn a_stream_image_is_recorded_at_the_drawing_origin_with_its_frame_fitted_to_the_frame_size() {
    let image = StreamImage {
        stream: StreamId::next(),
        generation: 7,
        size: PixelSize::new(256, 128),
    };
    let destination = Rect::new(10.dp(), 20.dp(), 64.dp(), 64.dp());
    // To fit the 256 by 128 image into the 64 by 64 box, contain scales the image by 1 / 4. Cover scales it by 1 / 2.
    for (fit, frame) in [
        (
            ImageFit::contain(),
            Rect::new(0.dp(), 16.dp(), 64.dp(), 32.dp()),
        ),
        (
            ImageFit::cover(),
            Rect::new((-32).dp(), 0.dp(), 128.dp(), 64.dp()),
        ),
    ] {
        let mut commands = Vec::new();
        Drawing::new(
            &mut commands,
            Rect::new(10.dp(), 20.dp(), 100.dp(), 100.dp()),
            &screen(TextScaling::default()),
        )
        .stream_image(&image, Rect::new(0.dp(), 0.dp(), 64.dp(), 64.dp()), fit);
        assert_eq!(
            commands,
            [DrawCommand::StreamImage {
                image,
                destination,
                frame
            }]
        );
    }
}

#[test]
fn resolving_a_style_scales_its_font_size_and_keeps_the_rest() -> TestResult {
    let style = TextStyle::new("Inter", 28.sp(), Color::WHITE)
        .weight(FontWeight::SEMI_BOLD)
        .letter_spacing(0.01.em());
    let resolved = style.resolve(&TextScaling::linear(1.5).required("scaling")?);
    assert_eq!(resolved.size, 42.dp());
    assert_eq!(
        (
            resolved.family.as_str(),
            resolved.weight,
            resolved.letter_spacing
        ),
        ("Inter", FontWeight::SEMI_BOLD, 0.01.em())
    );
    Ok(())
}

#[test]
fn canvas_text_follows_the_text_size_setting() -> TestResult {
    let mut commands = Vec::new();
    Drawing::new(
        &mut commands,
        Rect::new(10.dp(), 20.dp(), 50.dp(), 50.dp()),
        &screen(TextScaling::linear(1.5).required("scaling")?),
    )
    .text(
        "label",
        Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp()),
        TextStyle::new("Inter", 12.sp(), Color::WHITE),
    );
    let [DrawCommand::Text { style, rect, .. }] = commands.as_slice() else {
        return Err(TestError::Absent("one text command"));
    };
    assert_eq!(style.size, 18.dp());
    assert_eq!(*rect, Rect::new(10.dp(), 20.dp(), 40.dp(), 20.dp()));
    Ok(())
}

const fn at(x: f64, y: f64) -> Point {
    Point::new(Dp::new(x), Dp::new(y))
}

const fn line(x: f64, y: f64) -> Segment {
    Segment::Line(at(x, y))
}

static SHAPES: Icon = Icon {
    grid: Size::new(Dp::new(10.0), Dp::new(10.0)),
    paths: &[
        IconPath {
            start: at(0.0, 0.0),
            segments: &[line(10.0, 0.0), line(5.0, 10.0)],
            closed: true,
            stroke: Some(1.0),
        },
        IconPath {
            start: at(0.0, 10.0),
            segments: &[line(10.0, 10.0)],
            closed: false,
            stroke: Some(1.0),
        },
        IconPath {
            start: at(4.0, 4.0),
            segments: &[line(6.0, 4.0), line(6.0, 6.0)],
            closed: true,
            stroke: None,
        },
    ],
};

fn shapes(color: Color, filled: bool) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    Drawing::new(
        &mut commands,
        Rect::new(10.dp(), 20.dp(), 100.dp(), 100.dp()),
        &screen(TextScaling::default()),
    )
    .icon(
        &SHAPES,
        Rect::new(10.dp(), 20.dp(), 40.dp(), 20.dp()),
        color,
        filled,
    );
    commands
}

fn path(
    start: Point,
    lines: &[Point],
    closed: bool,
    color: Color,
    stroke: Option<Dp>,
) -> DrawCommand {
    DrawCommand::Path {
        start,
        segments: lines.iter().map(|point| Segment::Line(*point)).collect(),
        closed,
        brush: color.into(),
        stroke,
    }
}

#[test]
fn an_icon_scales_its_grid_into_the_middle_of_its_box_with_its_strokes() {
    let white = Color::WHITE;
    assert_eq!(
        shapes(white, false),
        [
            path(
                at(30.0, 40.0),
                &[at(50.0, 40.0), at(40.0, 60.0)],
                true,
                white,
                Some(2.dp())
            ),
            path(
                at(30.0, 60.0),
                &[at(50.0, 60.0)],
                false,
                white,
                Some(2.dp())
            ),
            path(
                at(38.0, 48.0),
                &[at(42.0, 48.0), at(42.0, 52.0)],
                true,
                white,
                None
            ),
        ]
    );
}

#[test]
fn a_filled_icon_fills_its_closed_paths_under_their_strokes() -> TestResult {
    let kinds = shapes(Color::WHITE, true)
        .into_iter()
        .map(|command| match command {
            DrawCommand::Path { closed, stroke, .. } => Ok((closed, stroke.is_some())),
            _ => Err(TestError::Absent("a path command")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        kinds,
        [(true, false), (true, true), (false, true), (true, false)]
    );
    Ok(())
}

#[test]
fn a_translucent_icon_paints_opaque_paths_inside_one_opacity_layer() {
    let commands = shapes(Color::WHITE.with_alpha(0.5), false);
    assert_eq!(
        commands.first(),
        Some(&DrawCommand::OpacityLayer {
            alpha: 0.5,
            bounds: Rect::new(19.dp(), 39.dp(), 42.dp(), 22.dp()),
        })
    );
    assert_eq!(commands.last(), Some(&DrawCommand::Restore));
    let inner = &commands[1..commands.len() - 1];
    assert_eq!(inner, shapes(Color::WHITE, false));
}

#[test]
fn an_icon_without_an_area_paints_nothing_and_a_path_off_its_grid_is_left_out() {
    static FLAT: Icon = Icon {
        grid: Size::new(Dp::ZERO, Dp::new(10.0)),
        paths: &[IconPath {
            start: at(0.0, 0.0),
            segments: &[line(10.0, 10.0)],
            closed: false,
            stroke: Some(1.0),
        }],
    };
    static STRAY: Icon = Icon {
        grid: Size::new(Dp::new(10.0), Dp::new(10.0)),
        paths: &[IconPath {
            start: at(0.0, 0.0),
            segments: &[line(f64::NAN, 10.0)],
            closed: false,
            stroke: Some(1.0),
        }],
    };
    let square = Rect::new(10.dp(), 20.dp(), 40.dp(), 20.dp());
    for icon in [&FLAT, &STRAY] {
        let mut commands = Vec::new();
        icon.paint(square, Color::WHITE, true, &mut commands);
        assert!(commands.is_empty(), "{icon:?}: {commands:?}");
    }
}

#[test]
fn an_icon_places_the_control_points_of_its_curves_like_its_other_points() {
    static CURVES: Icon = Icon {
        grid: Size::new(Dp::new(10.0), Dp::new(10.0)),
        paths: &[IconPath {
            start: at(0.0, 0.0),
            segments: &[
                Segment::Quadratic {
                    control: at(5.0, 0.0),
                    to: at(10.0, 5.0),
                },
                Segment::Cubic {
                    first_control: at(10.0, 8.0),
                    second_control: at(8.0, 10.0),
                    to: at(5.0, 10.0),
                },
            ],
            closed: false,
            stroke: Some(1.0),
        }],
    };
    let mut commands = Vec::new();
    CURVES.paint(
        Rect::new(10.dp(), 20.dp(), 20.dp(), 20.dp()),
        Color::WHITE,
        false,
        &mut commands,
    );
    assert_eq!(
        commands,
        [DrawCommand::Path {
            start: at(10.0, 20.0),
            segments: vec![
                Segment::Quadratic {
                    control: at(20.0, 20.0),
                    to: at(30.0, 30.0),
                },
                Segment::Cubic {
                    first_control: at(30.0, 36.0),
                    second_control: at(26.0, 40.0),
                    to: at(20.0, 40.0),
                },
            ],
            closed: false,
            brush: Color::WHITE.into(),
            stroke: Some(2.dp()),
        }]
    );
}

#[test]
fn a_path_without_points_draws_nothing() {
    let mut commands = Vec::new();
    Drawing::new(
        &mut commands,
        Rect::new(10.dp(), 20.dp(), 100.dp(), 100.dp()),
        &screen(TextScaling::default()),
    )
    .path(&[], true, Color::WHITE, None);
    assert!(commands.is_empty());
}
