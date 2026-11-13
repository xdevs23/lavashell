// SPDX-License-Identifier: GPL-3.0-only
//! The layout of a panel: a surface that the host places at the edges of an output, such as a bar or a dock.
use crate::geometry::Size;
use crate::units::{Dp, FiniteDp, NonNegativeDp, NumberError};
use crate::view::Proposal;

/// The shortest length of a panel along an axis that it does not fill, so that the panel stays visible.
const SHORTEST_LENGTH: NonNegativeDp = crate::constant!(NonNegativeDp::try_new(Dp::new(1.0)));

/// Where the host stacks a panel, relative to the windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerLevel {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Where a panel sits along one axis of the output, with a length of type `L` along that axis. The start edge is the
/// top or the left edge. The end edge is the bottom or the right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisPlacement<L = Extent> {
    /// Anchored to the start edge, with this length.
    Start(L),
    /// Anchored to the end edge, with this length.
    End(L),
    /// Anchored to neither edge, so the host centers the panel.
    Center(L),
    /// Anchored to both edges, across the whole distance between them.
    Fill,
}

impl<L> AxisPlacement<L> {
    /// The length of the panel along the axis, or `None` when the panel fills the axis.
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

    /// The placement with a whole length, where `content` is the length of the content along the axis.
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

/// The length of a panel along an axis that it does not fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    /// The length of the content along this axis. The length is measured again after the view of the panel changes.
    Content,
    Dp(NonNegativeDp),
}

impl Extent {
    /// # Errors
    ///
    /// Fails when `length` is negative or not finite.
    pub fn dp(length: Dp) -> Result<Self, NumberError> {
        NonNegativeDp::try_new(length).map(Self::Dp)
    }
}

/// The exclusive zone of a panel: the space at its anchored edge that the host reserves for the panel and does not
/// give to other surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExclusiveZone {
    /// Reserves this distance from the anchored edge. The panel is placed outside the space reserved by other
    /// panels.
    Reserve(NonNegativeDp),
    /// Reserves the length of the panel across its anchored edge.
    LayerExtent,
    /// Reserves nothing. The panel also extends under the space reserved by other panels.
    IgnoreOthers,
}

impl ExclusiveZone {
    /// # Errors
    ///
    /// Fails when `length` is negative or not finite.
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
    /// The size of the panel, or `None` when an axis fills or follows the content.
    #[must_use]
    pub fn fixed_size(&self) -> Option<Size> {
        let width = self.horizontal.fixed_length()?;
        let height = self.vertical.fixed_length()?;
        Some(Size::new(width.get(), height.get()))
    }

    /// Whether a length of the panel follows its content.
    #[must_use]
    pub fn follows_content(&self) -> bool {
        self.horizontal.follows_content() || self.vertical.follows_content()
    }

    /// The size proposal for measuring the content of the panel. The size proposal for an axis with a fixed length
    /// is that length. The size proposal for an axis that fills or follows the content is unbounded.
    #[must_use]
    pub fn content_proposal(&self) -> Proposal {
        let bound = |axis: AxisPlacement| axis.fixed_length().map(NonNegativeDp::get);
        Proposal {
            width: bound(self.horizontal),
            height: bound(self.vertical),
        }
    }
}

/// The size of the content of a panel, measured from its view.
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

/// The geometry of a panel that a platform sends to the host: a layout with every length resolved to a whole number
/// of dp, the unit in which hosts place panels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelGeometry {
    horizontal: AxisPlacement<NonNegativeDp>,
    vertical: AxisPlacement<NonNegativeDp>,
    exclusive_zone: ExclusiveZone,
    margins: LayerMargins,
}

impl PanelGeometry {
    /// The geometry of a panel with `layout`, whose content has the size `content`.
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
