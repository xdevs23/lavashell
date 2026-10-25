// SPDX-License-Identifier: GPL-3.0-only
use super::color::color;
use super::shape::{rect, rounded};
use super::{Check, Samples, TestError, TestResult, read};
use lavashell_core::color::{ColorInterpolation, Gamut, HuePath};
use lavashell_core::effects::{
    Backdrop, BlendMode, BorderPlacement, Glass, Shadow, SourceFilter, SourceFilterId, Stroke,
};
use lavashell_core::geometry::{Color, FinitePoint, Point};
use lavashell_core::gradient::{ColorStop, Gradient, GradientKind, TileMode};
use lavashell_core::scene::{
    Affine, Brush, DrawCommand, FontWeight, ParagraphDirection, Segment, TextAlignment, TextSpan,
    TextStyle,
};
use lavashell_core::shape::{Corners, Radius, Shape};
use lavashell_core::units::{Dp, DpExt, EmExt, FiniteDp, NonNegative, NonNegativeDp, NumberError};

pub(super) fn affine() -> Affine {
    Affine {
        linear: [1.5, 0.25, -0.5, 2.0],
        translation: Point::new(3.dp(), 7.dp()),
    }
}

fn shape() -> Shape {
    rounded().into()
}

fn style() -> TextStyle<Dp> {
    TextStyle {
        size: 15.dp(),
        color: color(),
        family: "Inter".into(),
        weight: FontWeight::SEMI_BOLD,
        max_lines: Some(2),
        letter_spacing: 0.25.em(),
        alignment: TextAlignment::Center,
        direction: ParagraphDirection::RightToLeft,
        spans: vec![TextSpan {
            range: 1..4,
            weight: Some(FontWeight::BOLD),
            italic: true,
            family: Some("Mono".into()),
        }],
    }
}

fn corners() -> Corners<Radius> {
    Corners::new(
        Radius::new(1.dp(), 2.dp()),
        Radius::circular(3.dp()),
        Radius::new(4.dp(), 5.dp()),
        Radius::circular(6.dp()),
    )
}

fn non_negative(dp: i32) -> Result<NonNegativeDp, NumberError> {
    NonNegativeDp::try_new(dp.dp())
}

fn backdrop() -> Result<Backdrop, NumberError> {
    let glass = Glass {
        refraction: non_negative(8)?,
        dispersion: non_negative(2)?,
        rim: NonNegative::try_new(0.5)?,
    };
    Ok(Backdrop {
        blur: non_negative(12)?,
        glass: Some(glass),
    })
}

fn shadow() -> Result<Shadow, NumberError> {
    Ok(Shadow {
        color: color(),
        offset: FinitePoint::try_new(Point::new(1.dp(), 4.dp()))?,
        blur: non_negative(10)?,
        spread: FiniteDp::try_new((-2).dp())?,
        inset: true,
    })
}

fn stroke() -> Result<Stroke, NumberError> {
    Ok(Stroke {
        color: color(),
        width: NonNegativeDp::try_new(1.5.dp())?,
        placement: BorderPlacement::Outside,
    })
}

fn effect_layer() -> Result<DrawCommand, NumberError> {
    let filter = SourceFilter {
        id: SourceFilterId(7),
        parameters: vec![0.5, 2.0],
        outset: non_negative(3)?,
    };
    Ok(DrawCommand::EffectLayer {
        bounds: rect(),
        blend: BlendMode::Screen,
        blur: non_negative(6)?,
        filter: Some(filter),
    })
}

fn circle() -> DrawCommand {
    DrawCommand::Circle {
        center: Point::new(30.dp(), 40.dp()),
        radius: 12.dp(),
        brush: color().into(),
    }
}

fn path() -> DrawCommand {
    DrawCommand::Path {
        start: Point::new(1.dp(), 2.dp()),
        segments: vec![Segment::Line(Point::new(3.dp(), 5.dp()))],
        closed: true,
        brush: color().into(),
        stroke: Some(2.dp()),
    }
}

fn text() -> DrawCommand {
    DrawCommand::Text {
        text: "Lisbon".into(),
        rect: rect(),
        style: style(),
    }
}

fn private_text() -> Result<DrawCommand, TestError> {
    Ok(DrawCommand::PrivateText {
        text: read(&[6, 115, 101, 99, 114, 101, 116, 3, 226, 128, 162])?,
        rect: rect(),
        style: style(),
    })
}

fn draw_commands() -> Result<Samples<DrawCommand>, TestError> {
    Ok(every_variant! {
        #[process_local(DrawCommand::Image { .. }, DrawCommand::StreamImage { .. })]
        DrawCommand::Save => DrawCommand::Save, [0];
        DrawCommand::Restore => DrawCommand::Restore, [1];
        DrawCommand::Clip { .. } => DrawCommand::Clip { rect: rect(), corners: corners() }, [
            2, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0,
            0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 20,
            64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 24, 64,
        ];
        DrawCommand::Opacity(_) => DrawCommand::Opacity(0.5), [
            3, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
        DrawCommand::OpacityLayer { .. } => DrawCommand::OpacityLayer {
            alpha: 0.75,
            bounds: rect(),
        }, [
            4, 0, 0, 0, 0, 0, 0, 232, 63, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0,
            0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64,
        ];
        DrawCommand::Transform(_) => DrawCommand::Transform(affine()), [
            5, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 224, 191, 0,
            0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28, 64,
        ];
        DrawCommand::EffectLayer { .. } => effect_layer()?, [
            6, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 14, 0, 0, 0, 0, 0, 0, 24, 64, 1, 7, 2, 0, 0, 0, 0, 0, 0, 224, 63,
            0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64,
        ];
        DrawCommand::Backdrop { .. } => DrawCommand::Backdrop {
            shape: shape(),
            effect: backdrop()?,
        }, [
            7, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0,
            0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0,
            0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 40, 64, 1, 0, 0, 0, 0, 0, 0,
            32, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
        DrawCommand::Shadow { .. } => DrawCommand::Shadow { shape: shape(), shadow: shadow()? }, [
            8, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0,
            0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0,
            0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215,
            163, 112, 61, 10, 199, 63, 0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0,
            0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0,
            0, 0, 192, 1,
        ];
        DrawCommand::Stroke { .. } => DrawCommand::Stroke { shape: shape(), stroke: stroke()? }, [
            9, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0,
            0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0,
            0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215,
            163, 112, 61, 10, 199, 63, 0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0,
            0, 0, 0, 0, 248, 63, 2,
        ];
        DrawCommand::Fill { .. } => DrawCommand::Fill { shape: shape(), brush: color().into() }, [
            10, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0,
            0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0,
            0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10,
            215, 163, 112, 61, 10, 199, 63, 0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63,
        ];
        DrawCommand::Circle { .. } => circle(), [
            13, 0, 0, 0, 0, 0, 0, 62, 64, 0, 0, 0, 0, 0, 0, 68, 64, 0, 0, 0, 0, 0, 0, 40, 64, 0, 0,
            215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163, 112, 61, 10, 199, 63, 0, 0, 0, 0, 0,
            64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63,
        ];
        DrawCommand::Path { .. } => path(), [
            14, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64, 1, 0, 0, 0, 0, 0, 0, 0, 8, 64,
            0, 0, 0, 0, 0, 0, 20, 64, 1, 0, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163,
            112, 61, 10, 199, 63, 0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 1, 0, 0, 0,
            0, 0, 0, 0, 64,
        ];
        DrawCommand::Text { .. } => text(), [
            15, 6, 76, 105, 115, 98, 111, 110, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64,
            0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 46, 64, 0,
            215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163, 112, 61, 10, 199, 63, 0, 0, 0, 0, 0,
            64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 5, 73, 110, 116, 101, 114, 0, 0, 0, 0, 0, 192,
            130, 64, 1, 2, 0, 0, 0, 0, 0, 0, 208, 63, 1, 1, 1, 1, 4, 1, 0, 0, 0, 0, 0, 224, 133, 64,
            1, 1, 4, 77, 111, 110, 111,
        ];
        DrawCommand::PrivateText { .. } => private_text()?, [
            16, 6, 115, 101, 99, 114, 101, 116, 3, 226, 128, 162, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0,
            0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0,
            0, 46, 64, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163, 112, 61, 10, 199, 63,
            0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 5, 73, 110, 116, 101, 114, 0, 0,
            0, 0, 0, 192, 130, 64, 1, 2, 0, 0, 0, 0, 0, 0, 208, 63, 1, 1, 1, 1, 4, 1, 0, 0, 0, 0, 0,
            224, 133, 64, 1, 1, 4, 77, 111, 110, 111,
        ];
    })
}

fn brushes() -> Samples<Brush> {
    let stops = vec![
        ColorStop {
            position: 0.25,
            color: color(),
        },
        ColorStop {
            position: 0.75,
            color: Color::encoded([0.1, 0.2, 0.3], 0.9, Gamut::DisplayP3),
        },
    ];
    let gradient = Gradient {
        kind: GradientKind::Radial {
            center: Point::new(5.dp(), 9.dp()),
            radius: 30.dp(),
        },
        stops,
        tile: TileMode::Mirror,
        interpolation: ColorInterpolation::Oklch(HuePath::Longer),
    };
    every_variant! {
        Brush::Solid(_) => Brush::Solid(color()), [
            0, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163, 112, 61, 10, 199, 63, 0, 0, 0,
            0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63,
        ];
        Brush::Gradient(_) => Brush::Gradient(gradient), [
            1, 1, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 34, 64, 0, 0, 0, 0, 0, 0, 62, 64, 2,
            0, 0, 0, 0, 0, 0, 208, 63, 0, 215, 163, 112, 61, 10, 215, 227, 63, 10, 215, 163, 112,
            61, 10, 199, 63, 0, 0, 0, 0, 0, 64, 111, 64, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0,
            0, 232, 63, 2, 1, 154, 153, 153, 153, 153, 153, 185, 63, 154, 153, 153, 153, 153, 153,
            201, 63, 51, 51, 51, 51, 51, 51, 211, 63, 205, 204, 204, 204, 204, 204, 236, 63, 2, 1,
            1,
        ];
    }
}

fn segments() -> Samples<Segment> {
    every_variant! {
        Segment::Line(_) => Segment::Line(Point::new(3.dp(), 5.dp())), [
            0, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 20, 64,
        ];
        Segment::Quadratic { .. } => Segment::Quadratic {
            control: Point::new(1.dp(), 2.dp()),
            to: Point::new(3.dp(), 4.dp()),
        }, [
            1, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0,
            0, 0, 0, 16, 64,
        ];
        Segment::Cubic { .. } => Segment::Cubic {
            first_control: Point::new(1.dp(), 2.dp()),
            second_control: Point::new(3.dp(), 4.dp()),
            to: Point::new(5.dp(), 6.dp()),
        }, [
            2, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0,
            0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 24, 64,
        ];
    }
}

fn text_alignments() -> Samples<TextAlignment> {
    every_variant! {
        TextAlignment::Start => TextAlignment::Start, [0];
        TextAlignment::Center => TextAlignment::Center, [1];
        TextAlignment::End => TextAlignment::End, [2];
    }
}

fn paragraph_directions() -> Samples<ParagraphDirection> {
    every_variant! {
        ParagraphDirection::LeftToRight => ParagraphDirection::LeftToRight, [0];
        ParagraphDirection::RightToLeft => ParagraphDirection::RightToLeft, [1];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&draw_commands()?)?;
    check.samples(&brushes())?;
    check.samples(&segments())?;
    check.samples(&text_alignments())?;
    check.samples(&paragraph_directions())
}
