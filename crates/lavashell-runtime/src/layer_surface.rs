// SPDX-License-Identifier: GPL-3.0-only
use crate::panels::{KeyboardPolicy, PanelOptions};
use crate::shape::WindowShape;
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::panel::{AxisPlacement, ExclusiveZone, LayerLayout, LayerLevel};
use lavashell_core::prelude::{HdrMetadata, OutputRequest};
use lavashell_platform::KeyboardMode;

/// A panel that the host places at the edges of an output, such as a bar or a dock.
#[derive(Clone)]
pub struct LayerSurface {
    pub composition_order: i32,
    pub focus_grab: Option<FocusGrab>,
    pub shape: Option<WindowShape>,
    pub namespace: String,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub level: LayerLevel,
    /// The layout of the panel on its output and when it receives the keyboard focus.
    pub panel: PanelOptions,
}

impl LayerSurface {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            composition_order: 0,
            namespace: namespace.into(),
            focus_grab: None,
            shape: None,
            output: OutputRequest::default(),
            hdr_metadata: None,
            background_blur: false,
            level: LayerLevel::Top,
            panel: PanelOptions::default(),
        }
    }

    /// Sets the focus grab of this layer, which limits input focus to the layer while `grab` is active, such as
    /// for a popover.
    #[must_use]
    pub fn focus_grab(mut self, grab: FocusGrab) -> Self {
        self.focus_grab = Some(grab);
        self
    }

    /// Sets the reactive state that contains the layout of the layer: the margins, the placement on both axes and the
    /// exclusive zone. A change of the state is sent to the host with the next frame of the layer.
    #[must_use]
    pub fn layout_state(mut self, state: lavashell_core::reactive::State<LayerLayout>) -> Self {
        self.panel.layout_state = Some(state);
        self
    }

    /// Sets the shape of the layer. The outline of the shape clips painting and sets the input region sent to the host.
    #[must_use]
    pub fn shape(mut self, shape: WindowShape) -> Self {
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
        self.panel.layout.horizontal = horizontal;
        self.panel.layout.vertical = vertical;
        self
    }

    /// Sets the order in which the app composes this layer among its surfaces in each frame. Smaller orders are
    /// composed first. The order does not change the stacking by the host. A layer that reads geometry
    /// published by the view of another surface gets a larger order, so that it reads the geometry of the same
    /// frame. Surfaces with equal orders are composed in the order of their creation. The default order is zero.
    #[must_use]
    pub fn composition_order(mut self, order: i32) -> Self {
        self.composition_order = order;
        self
    }

    #[must_use]
    pub fn exclusive_zone(mut self, zone: ExclusiveZone) -> Self {
        self.panel.layout.exclusive_zone = zone;
        self
    }

    /// Allows the keyboard focus on demand, under the condition of `keyboard_when` if one is set, or never.
    #[must_use]
    pub fn keyboard(mut self, enabled: bool) -> Self {
        self.panel.keyboard = match (enabled, self.panel.keyboard) {
            (false, _) => KeyboardPolicy::Off,
            (true, KeyboardPolicy::OnDemandWhen(active)) => KeyboardPolicy::OnDemandWhen(active),
            (true, KeyboardPolicy::Off | KeyboardPolicy::OnDemand) => KeyboardPolicy::OnDemand,
        };
        self
    }

    /// Allows the keyboard focus on demand only while `active` returns true. A change of `active` is sent to the host
    /// in the same commit as the next change of the layer geometry and content.
    #[must_use]
    pub fn keyboard_when(mut self, active: impl Fn() -> bool + 'static) -> Self {
        self.panel.keyboard = KeyboardPolicy::OnDemandWhen(std::rc::Rc::new(active));
        self
    }

    #[must_use]
    pub fn keyboard_enabled(&self) -> bool {
        self.panel.keyboard.mode() == KeyboardMode::OnDemand
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_the_keyboard_keeps_a_condition_set_before_it() {
        let waiting = LayerSurface::new("keyboard")
            .keyboard_when(|| false)
            .keyboard(true);
        let disabled = LayerSurface::new("keyboard")
            .keyboard_when(|| true)
            .keyboard(false);
        let enabled = LayerSurface::new("keyboard")
            .keyboard_when(|| true)
            .keyboard(true);
        assert_eq!(waiting.panel.keyboard.mode(), KeyboardMode::Off);
        assert_eq!(disabled.panel.keyboard.mode(), KeyboardMode::Off);
        assert_eq!(enabled.panel.keyboard.mode(), KeyboardMode::OnDemand);
    }
}
