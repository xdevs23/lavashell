// SPDX-License-Identifier: GPL-3.0-only
use crate::effects::{Backdrop, BlendMode, Shadow, SourceFilter, Stroke};
use crate::geometry::{Color, PixelSize, Point, Rect, Size, Viewport};
use crate::gradient::Gradient;
use crate::image::{Image, ImageFit, StreamImage};
use crate::local::Local;
use crate::preferences::TextScaling;
use crate::shape::{Corners, Radius, Shape};
use crate::units::{Dp, Em, NonNegativeDp, Sp};

mod font_weight;

pub use font_weight::FontWeight;

mod icon;

pub use icon::{Icon, IconPath};

mod segment;

pub use segment::Segment;
pub(crate) use segment::path_points;

mod drawing;

pub use drawing::Drawing;

pub mod tree;

mod transform;

pub(crate) use transform::Transform;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Brush {
    Solid(Color),
    Gradient(Gradient),
}

impl From<Gradient> for Brush {
    fn from(g: Gradient) -> Self {
        Self::Gradient(g)
    }
}

impl Brush {
    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(c),
            Self::Gradient(g) => Self::Gradient(g.translated(offset)),
        }
    }
}

impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Self::Solid(c)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ParagraphDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for crate::units::Sp {}

    impl Sealed for crate::units::Dp {}
}

/// The unit of the font size of a text style: [`Sp`], as applications give it, or [`Dp`], after
/// the text size setting applies.
pub trait FontSize: sealed::Sealed + Copy + std::fmt::Debug + PartialEq {}

impl FontSize for Sp {}

impl FontSize for Dp {}

/// The font, size, color and layout of a text. Applications give the font size in sp. Text is measured and drawn
/// with a style in dp made by [`TextStyle::resolve`]. A size without a unit does not compile:
/// ```compile_fail
/// # use lavashell_core::scene::TextStyle;
/// # use lavashell_core::geometry::Color;
/// let heading = TextStyle::new("Inter", 28.0, Color::WHITE);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextStyle<S: FontSize = Sp> {
    pub size: S,
    pub color: Color,
    pub family: String,
    pub weight: FontWeight,
    pub max_lines: Option<usize>,
    pub letter_spacing: Em,
    pub alignment: TextAlignment,
    pub direction: ParagraphDirection,
    /// Non-overlapping UTF-8 byte ranges for inline styling. Invalid ranges are ignored.
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSpan {
    pub range: std::ops::Range<usize>,
    pub weight: Option<FontWeight>,
    pub italic: bool,
    pub family: Option<String>,
}

/// The style of composed text, and the color of composed icons. Its root value is body text of the
/// platform: the platform text size and foreground color in the generic sans-serif family.
pub static TEXT_STYLE: Local<TextStyle> = Local::platform(|platform| {
    TextStyle::new("sans-serif", platform.text_size, platform.foreground)
});

impl TextStyle {
    /// Text of `family` at `size` in `color`, at the normal weight, without a cap on its lines.
    pub fn new(family: impl Into<String>, size: Sp, color: Color) -> Self {
        Self {
            size,
            color,
            family: family.into(),
            weight: FontWeight::NORMAL,
            max_lines: None,
            letter_spacing: Em::ZERO,
            alignment: TextAlignment::Start,
            direction: ParagraphDirection::LeftToRight,
            spans: Vec::new(),
        }
    }

    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[must_use]
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    #[must_use]
    pub fn size(mut self, size: Sp) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn letter_spacing(mut self, spacing: Em) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// This style with its font size converted to dp by the text size setting.
    #[must_use]
    pub fn resolve(self, scaling: &TextScaling) -> TextStyle<Dp> {
        TextStyle {
            size: scaling.to_dp(self.size),
            color: self.color,
            family: self.family,
            weight: self.weight,
            max_lines: self.max_lines,
            letter_spacing: self.letter_spacing,
            alignment: self.alignment,
            direction: self.direction,
            spans: self.spans,
        }
    }

    /// This style at the default text size setting, where one sp is one dp.
    #[must_use]
    pub fn resolve_unscaled(self) -> TextStyle<Dp> {
        self.resolve(&TextScaling::default())
    }
}

/// The mark drawn at the end of text cut by [`TextStyle::max_lines`].
pub const ELLIPSIS: &str = "…";

/// A linear map without unit, followed by a translation in dp. With `linear` as
/// `[a, b, c, d]`, the point (x, y) moves to (a x + b y, c x + d y) + `translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Affine {
    pub linear: [f64; 4],
    pub translation: Point,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        linear: [1.0, 0.0, 0.0, 1.0],
        translation: Point::new(Dp::ZERO, Dp::ZERO),
    };

    #[must_use]
    pub fn translate(offset: Point) -> Self {
        Self {
            linear: [1.0, 0.0, 0.0, 1.0],
            translation: offset,
        }
    }

    #[must_use]
    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            linear: [x, 0.0, 0.0, y],
            translation: Point::default(),
        }
    }

    /// A positive angle turns clockwise on screen, where y points down.
    #[must_use]
    pub fn rotate(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            linear: [cos, -sin, sin, cos],
            translation: Point::default(),
        }
    }

    /// This map followed by a move by `offset`.
    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        Self {
            translation: self.translation.offset(offset.x, offset.y),
            ..self
        }
    }

    /// `inner` followed by this map.
    #[must_use]
    pub fn then(self, inner: Self) -> Self {
        let [s00, s01, s10, s11] = self.linear;
        let [i00, i01, i10, i11] = inner.linear;
        Self {
            linear: [
                s00 * i00 + s01 * i10,
                s00 * i01 + s01 * i11,
                s10 * i00 + s11 * i10,
                s10 * i01 + s11 * i11,
            ],
            translation: self.map_point(inner.translation),
        }
    }

    #[must_use]
    pub fn map_point(self, point: Point) -> Point {
        let [a, b, c, d] = self.linear;
        Point::new(
            point.x * a + point.y * b + self.translation.x,
            point.x * c + point.y * d + self.translation.y,
        )
    }

    #[must_use]
    pub fn stretch(self) -> f64 {
        let [a, b, c, d] = self.linear;
        let squares = a * a + b * b + c * c + d * d;
        let determinant = a * d - b * c;
        let spread = (squares * squares - 4.0 * determinant * determinant).max(0.0);
        f64::midpoint(squares, spread.sqrt()).sqrt()
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DrawCommand {
    Save,
    Restore,
    Clip {
        rect: Rect,
        corners: Corners<Radius>,
    },
    Opacity(f64),
    OpacityLayer {
        alpha: f64,
        bounds: Rect,
    },
    Transform(Affine),
    EffectLayer {
        bounds: Rect,
        blend: BlendMode,
        blur: NonNegativeDp,
        filter: Option<SourceFilter>,
    },
    Backdrop {
        shape: Shape,
        effect: Backdrop,
    },
    Shadow {
        shape: Shape,
        shadow: Shadow,
    },
    Stroke {
        shape: Shape,
        stroke: Stroke,
    },
    Fill {
        shape: Shape,
        brush: Brush,
    },
    Image {
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::process_local"))]
        image: Image,
        destination: Rect,
        /// Where the whole image is placed, measured from the top left corner of `destination`.
        frame: Rect,
    },
    /// A frame of a stream. Its pixels exist only while the frame is painted, so a renderer must not cache them.
    StreamImage {
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::process_local"))]
        image: StreamImage,
        destination: Rect,
        /// Where the whole frame is placed, measured from the top left corner of `destination`.
        frame: Rect,
    },
    Circle {
        center: Point,
        radius: Dp,
        brush: Brush,
    },
    Path {
        start: Point,
        #[cfg_attr(
            feature = "serde",
            serde(deserialize_with = "crate::encoding::sequence")
        )]
        segments: Vec<Segment>,
        closed: bool,
        brush: Brush,
        stroke: Option<Dp>,
    },
    Text {
        text: String,
        rect: Rect,
        style: TextStyle<Dp>,
    },
    /// Protected text revealed by the application. A renderer must not cache or log the plain text.
    PrivateText {
        text: crate::protected_text::PrivateText,
        rect: Rect,
        style: TextStyle<Dp>,
    },
}

impl DrawCommand {
    /// Whether the command draws content that no cache may store, neither as a rendered image nor as the command:
    /// revealed protected text or a stream frame.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::PrivateText { .. } | Self::StreamImage { .. })
    }

    /// Draws `image` into `destination`, in the coordinates of the commands around it, as `fit`
    /// places it. Vw and vh are percent of `viewport`.
    #[must_use]
    pub fn image(image: &Image, destination: Rect, fit: ImageFit, viewport: Size) -> Self {
        let (width, height) = image.dimensions();
        let pixels = PixelSize::new(width, height);
        Self::Image {
            image: image.clone(),
            destination,
            frame: fit.resolve(pixels, destination.size(), viewport),
        }
    }
}

/// The viewport and the text size setting where a canvas was composed.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawContext {
    /// Vw and vh are percent of its size.
    pub viewport: Viewport,
    pub text_scaling: TextScaling,
}

impl DrawContext {
    /// A context in `viewport` with the default text size setting.
    #[must_use]
    pub fn new(viewport: Viewport) -> Self {
        Self {
            viewport,
            text_scaling: TextScaling::default(),
        }
    }
}

#[cfg(test)]
mod tests;
