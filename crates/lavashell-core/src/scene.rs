// SPDX-License-Identifier: GPL-3.0-only
use crate::effects::{Backdrop, BlendMode, Shadow, SourceFilter, Stroke};
use crate::geometry::{Color, PixelSize, Point, Rect, Size, Viewport};
use crate::image::{Image, ImageFit, StreamImage};
use crate::local::Local;
use crate::preferences::TextScaling;
use crate::shape::{Corners, Radius, Shape};
use crate::units::{Dp, Em, NonNegativeDp, Sp};

mod font_weight;

pub use font_weight::FontWeight;

mod icon;

pub use icon::{Icon, IconPath, IconStyle};

mod segment;

pub use segment::Segment;
pub(crate) use segment::path_points;

mod drawing;

pub use drawing::Drawing;

mod material;

pub(crate) use material::MaterialCommands;

mod builder;
#[cfg(test)]
mod builder_tests;
#[cfg(test)]
pub(crate) mod test_sink;

pub use builder::{ChangeSink, SceneBuilder, SinkId};

pub mod tree;

mod transform;

pub(crate) use transform::Transform;

pub use crate::brush::Brush;

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

impl TextAlignment {
    pub(crate) fn share_of_widening_on_the_left(self, direction: ParagraphDirection) -> f64 {
        match (self, direction) {
            (Self::Start, ParagraphDirection::LeftToRight)
            | (Self::End, ParagraphDirection::RightToLeft) => 0.0,
            (Self::Center, _) => 0.5,
            (Self::Start, ParagraphDirection::RightToLeft)
            | (Self::End, ParagraphDirection::LeftToRight) => 1.0,
        }
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for crate::units::Sp {}

    impl Sealed for crate::units::Dp {}
}

pub trait FontSize: sealed::Sealed + Copy + std::fmt::Debug + PartialEq {}

impl FontSize for Sp {}

impl FontSize for Dp {}

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

pub static TEXT_STYLE: Local<TextStyle> = Local::platform(|platform| {
    TextStyle::new("sans-serif", platform.text_size, platform.foreground)
});

impl TextStyle {
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

    #[must_use]
    pub fn resolve_unscaled(self) -> TextStyle<Dp> {
        self.resolve(&TextScaling::default())
    }
}

pub const ELLIPSIS: &str = "…";

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Affine {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
    pub translation: Point,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
        translation: Point::new(Dp::ZERO, Dp::ZERO),
    };

    #[must_use]
    pub fn translate(offset: Point) -> Self {
        Self {
            translation: offset,
            ..Self::IDENTITY
        }
    }

    #[must_use]
    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            xx: x,
            yy: y,
            ..Self::IDENTITY
        }
    }

    #[must_use]
    pub fn rotate(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            xx: cos,
            xy: -sin,
            yx: sin,
            yy: cos,
            translation: Point::default(),
        }
    }

    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        Self {
            translation: self.translation.offset(offset.x, offset.y),
            ..self
        }
    }

    #[must_use]
    pub fn then(self, inner: Self) -> Self {
        Self {
            xx: self.xx * inner.xx + self.xy * inner.yx,
            xy: self.xx * inner.xy + self.xy * inner.yy,
            yx: self.yx * inner.xx + self.yy * inner.yx,
            yy: self.yx * inner.xy + self.yy * inner.yy,
            translation: self.map_point(inner.translation),
        }
    }

    #[must_use]
    pub fn map_point(self, point: Point) -> Point {
        Point::new(
            point.x * self.xx + point.y * self.xy + self.translation.x,
            point.x * self.yx + point.y * self.yy + self.translation.y,
        )
    }

    #[must_use]
    pub fn stretch(self) -> f64 {
        let Self { xx, xy, yx, yy, .. } = self;
        let squares = xx * xx + xy * xy + yx * yx + yy * yy;
        let determinant = xx * yy - xy * yx;
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
        frame: Rect,
    },
    StreamImage {
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::process_local"))]
        image: StreamImage,
        destination: Rect,
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
    PrivateText {
        text: crate::protected_text::PrivateText,
        rect: Rect,
        style: TextStyle<Dp>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveStackChange {
    Push,
    Pop,
    Unchanged,
}

impl DrawCommand {
    pub(crate) fn save_stack_change(&self) -> SaveStackChange {
        match self {
            Self::Save
            | Self::Opacity(_)
            | Self::OpacityLayer { .. }
            | Self::EffectLayer { .. } => SaveStackChange::Push,
            Self::Restore => SaveStackChange::Pop,
            Self::Clip { .. }
            | Self::Transform(_)
            | Self::Backdrop { .. }
            | Self::Shadow { .. }
            | Self::Stroke { .. }
            | Self::Fill { .. }
            | Self::Image { .. }
            | Self::StreamImage { .. }
            | Self::Circle { .. }
            | Self::Path { .. }
            | Self::Text { .. }
            | Self::PrivateText { .. } => SaveStackChange::Unchanged,
        }
    }

    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::PrivateText { .. } | Self::StreamImage { .. })
    }

    #[must_use]
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Self::Save
                | Self::Restore
                | Self::Opacity(_)
                | Self::OpacityLayer { .. }
                | Self::Transform(_)
                | Self::Clip { .. }
                | Self::EffectLayer { .. }
        )
    }

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

#[derive(Clone, Debug, PartialEq)]
pub struct DrawContext {
    pub viewport: Viewport,
    pub text_scaling: TextScaling,
}

impl DrawContext {
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
