// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests;

use crate::geometry::{Length, PixelSize, PlainPoint, Rect, Size};
use crate::units::Dp;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageFit {
    Cover(ImagePlacement),
    Contain(ImagePlacement),
    Stretch,
    Frame(ImageFrame),
}

impl ImageFit {
    #[must_use]
    pub fn cover() -> Self {
        Self::Cover(ImagePlacement::default())
    }

    #[must_use]
    pub fn contain() -> Self {
        Self::Contain(ImagePlacement::default())
    }

    pub(crate) fn resolve(self, pixels: PixelSize, box_size: Size, viewport: Size) -> Rect {
        let (width, height) = (f64::from(pixels.width), f64::from(pixels.height));
        let scaled = |scale: Dp| Size::new(scale * width, scale * height);
        match self {
            Self::Cover(placement) => {
                let scale_to_cover = (box_size.width / width).max(box_size.height / height);
                placement.resolve(scaled(scale_to_cover), box_size, viewport)
            }
            Self::Contain(placement) => {
                let scale_to_contain = (box_size.width / width).min(box_size.height / height);
                placement.resolve(scaled(scale_to_contain), box_size, viewport)
            }
            Self::Stretch => Rect::from_size(box_size),
            Self::Frame(frame) => frame.resolve(box_size, viewport),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImagePlacement {
    pub anchor: Anchor,
    pub x: Length,
    pub y: Length,
}

impl ImagePlacement {
    #[must_use]
    pub fn at(anchor: Anchor) -> Self {
        Self {
            anchor,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn offset(self, x: impl Into<Length>, y: impl Into<Length>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            ..self
        }
    }

    fn resolve(self, image: Size, box_size: Size, viewport: Size) -> Rect {
        let anchor = self.anchor.fractions();
        let (x, y) = along_axes(self.x, self.y, box_size, viewport);
        Rect::new(
            (box_size.width - image.width) * anchor.x + x,
            (box_size.height - image.height) * anchor.y + y,
            image.width,
            image.height,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    #[must_use]
    pub fn fractions(self) -> PlainPoint {
        let (x, y) = match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        };
        PlainPoint::new(x, y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageFrame {
    pub x: Length,
    pub y: Length,
    pub width: Length,
    pub height: Length,
}

impl ImageFrame {
    pub fn new(
        x: impl Into<Length>,
        y: impl Into<Length>,
        width: impl Into<Length>,
        height: impl Into<Length>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            width: width.into(),
            height: height.into(),
        }
    }

    fn resolve(self, box_size: Size, viewport: Size) -> Rect {
        let (x, y) = along_axes(self.x, self.y, box_size, viewport);
        let (width, height) = along_axes(self.width, self.height, box_size, viewport);
        Rect::new(x, y, width, height)
    }
}

fn along_axes(horizontal: Length, vertical: Length, box_size: Size, viewport: Size) -> (Dp, Dp) {
    (
        horizontal.to_dp(box_size.width, viewport),
        vertical.to_dp(box_size.height, viewport),
    )
}
