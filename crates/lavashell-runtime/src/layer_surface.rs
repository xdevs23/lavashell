// SPDX-License-Identifier: GPL-3.0-only
use crate::panels::{KeyboardPolicy, PanelOptions};
use crate::shape::WindowShape;
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::panel::{AxisPlacement, ExclusiveZone, LayerLayout, LayerLevel};
use lavashell_core::prelude::{HdrMetadata, OutputRequest};
use lavashell_platform::KeyboardMode;

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

    #[must_use]
    pub fn focus_grab(mut self, grab: FocusGrab) -> Self {
        self.focus_grab = Some(grab);
        self
    }

    #[must_use]
    pub fn layout_state(mut self, state: lavashell_core::reactive::State<LayerLayout>) -> Self {
        self.panel.layout_state = Some(state);
        self
    }

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

    #[must_use]
    pub fn keyboard(mut self, enabled: bool) -> Self {
        self.panel.keyboard = match (enabled, self.panel.keyboard) {
            (false, _) => KeyboardPolicy::Off,
            (true, KeyboardPolicy::OnDemandWhen(active)) => KeyboardPolicy::OnDemandWhen(active),
            (true, KeyboardPolicy::Off | KeyboardPolicy::OnDemand) => KeyboardPolicy::OnDemand,
        };
        self
    }

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
mod tests;
