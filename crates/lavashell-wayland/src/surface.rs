// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Point, Size};
use lavashell_core::prelude::{HdrMetadata, OutputRequest};
use lavashell_core::units::{Dp, FiniteDp, NonNegativeDp, NumberError};

pub(crate) type DragRegion = std::rc::Rc<dyn Fn(Point, Size) -> bool>;

#[derive(Clone)]
pub struct Window {
    pub title: String,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub transparent: bool,
    pub shape: Option<crate::WindowShape>,
    pub(crate) drag_region: Option<DragRegion>,
    /// The size of the window when it opens, or `None` for the size measured by the view without a size proposal.
    pub size: Option<Size>,
    /// The xdg app id, which matches the desktop entry of the application.
    pub app_id: String,
    pub min_size: Option<Size>,
    pub fullscreen: bool,
}

impl Window {
    pub fn new(title: impl Into<String>, app_id: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            output: OutputRequest::default(),
            hdr_metadata: None,
            background_blur: false,
            transparent: false,
            shape: None,
            drag_region: None,
            size: None,
            app_id: app_id.into(),
            min_size: None,
            fullscreen: false,
        }
    }

    #[must_use]
    pub fn output(mut self, output: OutputRequest) -> Self {
        self.output = output;
        self
    }

    /// Asks the compositor to blur what is behind the whole surface. A compositor without background blur shows the
    /// transparent surface without blur. Backdrop filters inside the scene work independently of this setting.
    #[must_use]
    pub fn background_blur(mut self, enabled: bool) -> Self {
        self.background_blur = enabled;
        self
    }

    #[must_use]
    pub fn hdr_metadata(mut self, metadata: HdrMetadata) -> Self {
        self.hdr_metadata = Some(metadata);
        self
    }

    #[must_use]
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Clips the window to the outline of `shape`. Pointer and touch events outside the outline pass through to
    /// the surfaces below. A shaped window gets a transparent surface and draws its decorations itself.
    #[must_use]
    pub fn shape(mut self, shape: crate::WindowShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the region in which a press of the primary button starts a window move by the compositor. `region`
    /// returns whether a point in logical pixels belongs to the region of a window with the given size. The region
    /// usually covers the header of the window without the interactive controls in it.
    #[must_use]
    pub fn drag_region(mut self, region: impl Fn(Point, Size) -> bool + 'static) -> Self {
        self.drag_region = Some(std::rc::Rc::new(region));
        self
    }

    #[must_use]
    pub fn size(mut self, width: Dp, height: Dp) -> Self {
        self.size = Some(Size::new(width, height));
        self
    }

    #[must_use]
    pub fn min_size(mut self, width: Dp, height: Dp) -> Self {
        self.min_size = Some(Size::new(width, height));
        self
    }

    /// Opens the window fullscreen. Some compositors enable adaptive sync only for fullscreen windows.
    #[must_use]
    pub fn fullscreen(mut self, enabled: bool) -> Self {
        self.fullscreen = enabled;
        self
    }
}

#[derive(Clone, Copy)]
pub enum LayerLevel {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone)]
pub struct LayerSurface {
    pub composition_order: i32,
    pub focus_grab: Option<crate::FocusGrab>,
    pub shape: Option<crate::WindowShape>,
    pub layout_state: Option<lavashell_core::reactive::State<LayerLayout>>,
    pub namespace: String,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub level: LayerLevel,
    pub horizontal: AxisPlacement,
    pub vertical: AxisPlacement,
    pub exclusive_zone: ExclusiveZone,
    pub keyboard: bool,
    pub(crate) keyboard_active: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

pub(crate) struct Keyboard {
    enabled: bool,
    active: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

impl Keyboard {
    pub fn enabled(&self) -> bool {
        self.enabled && self.active.as_ref().is_none_or(|active| active())
    }
}

/// Where a layer sits along one axis of the output. The start edge is the top or the left edge. The end edge is the
/// bottom or the right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisPlacement {
    /// Anchored to the start edge, with this length.
    Start(Extent),
    /// Anchored to the end edge, with this length.
    End(Extent),
    /// Anchored to neither edge, so the compositor centers the layer.
    Center(Extent),
    /// Anchored to both edges, across the whole distance between them.
    Fill,
}

impl AxisPlacement {
    /// The length of the layer along the axis, or `None` when the layer fills the axis.
    #[must_use]
    pub fn extent(self) -> Option<Extent> {
        match self {
            Self::Start(extent) | Self::End(extent) | Self::Center(extent) => Some(extent),
            Self::Fill => None,
        }
    }
}

/// The length of a layer along an axis that it does not fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    /// The length that the content measures along this axis, measured again after the view of the layer changes.
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

/// The exclusive zone of a layer: the space at its anchored edge that the compositor reserves for the layer and
/// does not give to other surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExclusiveZone {
    /// Reserves this distance from the anchored edge. The layer is placed outside the space reserved by other
    /// layers.
    Reserve(NonNegativeDp),
    /// Reserves the length of the layer along the axis perpendicular to its anchored edge.
    LayerExtent,
    /// Reserves nothing. The layer also extends under the space reserved by other layers.
    IgnoreOthers,
}

impl ExclusiveZone {
    /// # Errors
    ///
    /// Fails when `length` is negative or not finite.
    pub fn reserve(length: Dp) -> Result<Self, NumberError> {
        NonNegativeDp::try_new(length).map(Self::Reserve)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerMargins {
    pub top: FiniteDp,
    pub right: FiniteDp,
    pub bottom: FiniteDp,
    pub left: FiniteDp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerLayout {
    pub margins: LayerMargins,
    pub horizontal: AxisPlacement,
    pub vertical: AxisPlacement,
    pub exclusive_zone: ExclusiveZone,
}

impl LayerLayout {
    /// The size of the layer, or `None` when an axis fills or follows the content.
    #[must_use]
    pub fn fixed_size(&self) -> Option<Size> {
        match (self.horizontal.extent(), self.vertical.extent()) {
            (Some(Extent::Dp(width)), Some(Extent::Dp(height))) => {
                Some(Size::new(width.get(), height.get()))
            }
            _ => None,
        }
    }
}

impl LayerSurface {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            composition_order: 0,
            namespace: namespace.into(),
            focus_grab: None,
            shape: None,
            layout_state: None,
            output: OutputRequest::default(),
            hdr_metadata: None,
            background_blur: false,
            level: LayerLevel::Top,
            horizontal: AxisPlacement::Fill,
            vertical: AxisPlacement::Start(Extent::Content),
            exclusive_zone: ExclusiveZone::LayerExtent,
            keyboard: false,
            keyboard_active: None,
        }
    }

    /// Sets the focus grab of this layer, which limits input focus to the layer while `grab` is active, such as
    /// for a popover.
    #[must_use]
    pub fn focus_grab(mut self, grab: crate::FocusGrab) -> Self {
        self.focus_grab = Some(grab);
        self
    }

    /// Sets the reactive state that contains the layout of the layer: its margins, its placement on both axes and
    /// its exclusive zone. A change of the state is sent to the compositor with the next frame of the layer.
    #[must_use]
    pub fn layout_state(mut self, state: lavashell_core::reactive::State<LayerLayout>) -> Self {
        self.layout_state = Some(state);
        self
    }

    pub(crate) fn current_layout(&self) -> LayerLayout {
        self.layout_state.as_ref().map_or_else(
            || LayerLayout {
                margins: LayerMargins::default(),
                horizontal: self.horizontal,
                vertical: self.vertical,
                exclusive_zone: self.exclusive_zone,
            },
            lavashell_core::reactive::State::get,
        )
    }

    /// Sets the shape of the layer. Its outline clips painting and sets the input region sent to the compositor.
    #[must_use]
    pub fn shape(mut self, shape: crate::WindowShape) -> Self {
        self.shape = Some(shape);
        self
    }

    #[must_use]
    pub fn output(mut self, output: OutputRequest) -> Self {
        self.output = output;
        self
    }

    #[must_use]
    pub fn background_blur(mut self, enabled: bool) -> Self {
        self.background_blur = enabled;
        self
    }

    #[must_use]
    pub fn hdr_metadata(mut self, metadata: HdrMetadata) -> Self {
        self.hdr_metadata = Some(metadata);
        self
    }

    #[must_use]
    pub fn level(mut self, level: LayerLevel) -> Self {
        self.level = level;
        self
    }

    #[must_use]
    pub fn placement(mut self, horizontal: AxisPlacement, vertical: AxisPlacement) -> Self {
        self.horizontal = horizontal;
        self.vertical = vertical;
        self
    }

    /// Sets the order in which the app composes this layer among its surfaces in each frame. Smaller orders are
    /// composed first. The order does not change the stacking by the compositor. A layer that reads geometry
    /// published by the view of another surface gets a larger order, so that it reads the geometry of the same
    /// frame. Surfaces with equal orders are composed in the order of their creation. The default order is zero.
    #[must_use]
    pub fn composition_order(mut self, order: i32) -> Self {
        self.composition_order = order;
        self
    }

    #[must_use]
    pub fn exclusive_zone(mut self, zone: ExclusiveZone) -> Self {
        self.exclusive_zone = zone;
        self
    }

    #[must_use]
    pub fn keyboard(mut self, enabled: bool) -> Self {
        self.keyboard = enabled;
        self
    }

    /// Allows keyboard focus only while `active` returns true and `keyboard` is enabled. A change of `active` is
    /// sent to the compositor in the same commit as the next change of the layer geometry and content.
    #[must_use]
    pub fn keyboard_when(mut self, active: impl Fn() -> bool + 'static) -> Self {
        self.keyboard_active = Some(std::rc::Rc::new(active));
        self
    }

    #[must_use]
    pub fn keyboard_enabled(&self) -> bool {
        self.keyboard_configuration().enabled()
    }

    pub(crate) fn keyboard_configuration(&self) -> Keyboard {
        Keyboard {
            enabled: self.keyboard,
            active: self.keyboard_active.clone(),
        }
    }
}
