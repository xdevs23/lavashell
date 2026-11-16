// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Size;
use crate::units::{Dp, FiniteDp, NonNegativeDp, NumberError};
use crate::view::Proposal;

const SHORTEST_LENGTH: NonNegativeDp = crate::constant!(NonNegativeDp::try_new(Dp::new(1.0)));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerLevel {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisPlacement<L = Extent> {
    Start(L),
    End(L),
    Center(L),
    Fill,
}

impl<L> AxisPlacement<L> {
    #[must_use]
    pub fn extent(self) -> Option<L> {
        match self {
            Self::Start(length) | Self::End(length) | Self::Center(length) => Some(length),
            Self::Fill => None,
        }
    }

    fn map_length<M>(self, convert: impl FnOnce(L) -> M) -> AxisPlacement<M> {
        match self {
            Self::Start(length) => AxisPlacement::Start(convert(length)),
            Self::End(length) => AxisPlacement::End(convert(length)),
            Self::Center(length) => AxisPlacement::Center(convert(length)),
            Self::Fill => AxisPlacement::Fill,
        }
    }
}

impl AxisPlacement {
    fn fixed_length(self) -> Option<NonNegativeDp> {
        match self.extent() {
            Some(Extent::Dp(length)) => Some(length),
            Some(Extent::Content) | None => None,
        }
    }

    fn follows_content(self) -> bool {
        self.extent() == Some(Extent::Content)
    }

    fn resolve(self, content: NonNegativeDp) -> AxisPlacement<NonNegativeDp> {
        self.map_length(|extent| {
            let length = match extent {
                Extent::Content => content,
                Extent::Dp(length) => length,
            };
            length.round().max(SHORTEST_LENGTH)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    Content,
    Dp(NonNegativeDp),
}

impl Extent {
    pub fn dp(length: Dp) -> Result<Self, NumberError> {
        NonNegativeDp::try_new(length).map(Self::Dp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExclusiveZone {
    Reserve(NonNegativeDp),
    LayerExtent,
    IgnoreOthers,
}

impl ExclusiveZone {
    pub fn reserve(length: Dp) -> Result<Self, NumberError> {
        NonNegativeDp::try_new(length).map(Self::Reserve)
    }

    fn round(self) -> Self {
        match self {
            Self::Reserve(length) => Self::Reserve(length.round()),
            Self::LayerExtent | Self::IgnoreOthers => self,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerMargins {
    pub top: FiniteDp,
    pub right: FiniteDp,
    pub bottom: FiniteDp,
    pub left: FiniteDp,
}

impl LayerMargins {
    fn round(self) -> Self {
        Self {
            top: self.top.round(),
            right: self.right.round(),
            bottom: self.bottom.round(),
            left: self.left.round(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerLayout {
    pub margins: LayerMargins,
    pub horizontal: AxisPlacement,
    pub vertical: AxisPlacement,
    pub exclusive_zone: ExclusiveZone,
}

impl LayerLayout {
    #[must_use]
    pub fn fixed_size(&self) -> Option<Size> {
        let width = self.horizontal.fixed_length()?;
        let height = self.vertical.fixed_length()?;
        Some(Size::new(width.get(), height.get()))
    }

    #[must_use]
    pub fn follows_content(&self) -> bool {
        self.horizontal.follows_content() || self.vertical.follows_content()
    }

    #[must_use]
    pub fn content_proposal(&self) -> Proposal {
        let bound = |axis: AxisPlacement| axis.fixed_length().map(NonNegativeDp::get);
        Proposal {
            width: bound(self.horizontal),
            height: bound(self.vertical),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentSize {
    pub width: NonNegativeDp,
    pub height: NonNegativeDp,
}

impl TryFrom<Size> for ContentSize {
    type Error = NumberError;

    fn try_from(size: Size) -> Result<Self, NumberError> {
        let (width, height) = size.to_non_negative()?;
        Ok(Self { width, height })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelGeometry {
    horizontal: AxisPlacement<NonNegativeDp>,
    vertical: AxisPlacement<NonNegativeDp>,
    exclusive_zone: ExclusiveZone,
    margins: LayerMargins,
}

impl PanelGeometry {
    #[must_use]
    pub fn of(layout: LayerLayout, content: ContentSize) -> Self {
        Self {
            horizontal: layout.horizontal.resolve(content.width),
            vertical: layout.vertical.resolve(content.height),
            exclusive_zone: layout.exclusive_zone.round(),
            margins: layout.margins.round(),
        }
    }

    #[must_use]
    pub fn horizontal(&self) -> AxisPlacement<NonNegativeDp> {
        self.horizontal
    }

    #[must_use]
    pub fn vertical(&self) -> AxisPlacement<NonNegativeDp> {
        self.vertical
    }

    #[must_use]
    pub fn exclusive_zone(&self) -> ExclusiveZone {
        self.exclusive_zone
    }

    #[must_use]
    pub fn margins(&self) -> LayerMargins {
        self.margins
    }
}

#[cfg(test)]
mod tests;
