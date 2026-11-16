// SPDX-License-Identifier: GPL-3.0-only
use crate::registry::Subsystem;
use crate::run::RuntimeSetup;
use crate::surface_records::measure_content;
use crate::view_actor::ViewError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins, PanelGeometry,
};
use lavashell_core::reactive::State;
use lavashell_core::surface::{SurfaceChange, SurfaceField, SurfaceId, SurfaceState};
use lavashell_core::view::{TextMeasurer, View};
use lavashell_platform::{Capability, KeyboardMode};
use std::collections::BTreeMap;
use std::rc::Rc;

mod error;
mod per_output;

pub use error::Error;
pub use per_output::PanelsPerOutput;

inventory::submit! { Subsystem { capability: Capability::Panels, used: shows_panels } }

fn shows_panels(setup: &RuntimeSetup) -> bool {
    setup.panels_per_output.is_some() || setup.panels().next().is_some()
}

#[derive(Clone)]
pub enum KeyboardPolicy {
    Off,
    OnDemand,
    OnDemandWhen(Rc<dyn Fn() -> bool>),
}

impl KeyboardPolicy {
    #[must_use]
    pub fn mode(&self) -> KeyboardMode {
        match self {
            Self::OnDemand => KeyboardMode::OnDemand,
            Self::OnDemandWhen(active) if active() => KeyboardMode::OnDemand,
            Self::Off | Self::OnDemandWhen(_) => KeyboardMode::Off,
        }
    }
}

#[derive(Clone)]
pub struct PanelOptions {
    pub layout: LayerLayout,
    pub layout_state: Option<State<LayerLayout>>,
    pub keyboard: KeyboardPolicy,
}

impl Default for PanelOptions {
    fn default() -> Self {
        Self {
            layout: LayerLayout {
                margins: LayerMargins::default(),
                horizontal: AxisPlacement::Fill,
                vertical: AxisPlacement::Start(Extent::Content),
                exclusive_zone: ExclusiveZone::LayerExtent,
            },
            layout_state: None,
            keyboard: KeyboardPolicy::Off,
        }
    }
}

impl PanelOptions {
    #[must_use]
    pub fn current_layout(&self) -> LayerLayout {
        (self.layout_state.as_ref()).map_or(self.layout, State::get)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelContent {
    pub size: Size,
    pub input: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentState {
    Current,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Transaction {
    Settled,
    Committed,
    Committing,
    NeedsFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct PanelUpdate {
    pub keyboard: Option<KeyboardMode>,
    pub transaction: Transaction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelRequests {
    geometry: Option<PanelGeometry>,
    keyboard: KeyboardMode,
}

pub struct Panel {
    options: PanelOptions,
    content: ContentSize,
    geometry: PanelGeometry,
    keyboard_mode: KeyboardMode,
    applied: Option<PanelRequests>,
    committing: Option<PanelRequests>,
    shown: Option<PanelContent>,
}

impl Panel {
    pub fn new(
        options: PanelOptions,
        view: &mut View,
        text: &mut impl TextMeasurer,
    ) -> Result<Self, ViewError> {
        let layout = options.current_layout();
        let content = measure_panel_content(view, layout, text)?;
        Ok(Self {
            geometry: PanelGeometry::of(layout, content),
            keyboard_mode: options.keyboard.mode(),
            applied: None,
            committing: None,
            shown: None,
            content,
            options,
        })
    }

    #[must_use]
    pub fn geometry(&self) -> PanelGeometry {
        self.geometry
    }

    #[must_use]
    pub fn keyboard_mode(&self) -> KeyboardMode {
        self.keyboard_mode
    }

    #[must_use]
    pub fn keyboard_allowed(&self) -> bool {
        self.options.keyboard.mode() == KeyboardMode::OnDemand
    }

    #[must_use]
    pub fn keyboard_granted(&self) -> bool {
        !self.pending() && self.keyboard_mode == KeyboardMode::OnDemand && self.keyboard_allowed()
    }

    #[must_use]
    pub fn input_enabled(&self) -> bool {
        self.shown.is_some_and(|content| content.input)
    }

    #[must_use]
    pub fn content_current(&self, size: Size) -> bool {
        self.shown.is_some_and(|content| content.size == size)
    }

    fn update(&mut self) -> Option<KeyboardMode> {
        self.geometry = PanelGeometry::of(self.options.current_layout(), self.content);
        let keyboard = self.options.keyboard.mode();
        if keyboard == self.keyboard_mode {
            return None;
        }
        self.keyboard_mode = keyboard;
        Some(keyboard)
    }

    fn requested(&self) -> PanelRequests {
        PanelRequests {
            geometry: Some(self.geometry),
            keyboard: self.keyboard_mode,
        }
    }

    fn pending(&self) -> bool {
        self.applied != Some(self.requested())
    }

    fn apply(&mut self, requests: PanelRequests) {
        if self.committing == Some(requests) {
            self.committing = None;
        }
        self.applied = Some(requests);
    }
}

#[derive(Default)]
pub struct PanelRecords {
    records: BTreeMap<SurfaceId, Panel>,
}

impl PanelRecords {
    pub fn open(&mut self, surface: SurfaceId, panel: Panel) {
        let _replaced_panel = self.records.insert(surface, panel);
    }

    pub fn forget(&mut self, surface: SurfaceId) {
        let _removed_panel = self.records.remove(&surface);
    }

    #[must_use]
    pub fn get(&self, surface: SurfaceId) -> Option<&Panel> {
        self.records.get(&surface)
    }

    pub fn measure(
        &mut self,
        surface: SurfaceId,
        view: &mut View,
        text: &mut impl TextMeasurer,
    ) -> Result<(), ViewError> {
        if let Some(panel) = self.records.get_mut(&surface) {
            let layout = panel.options.current_layout();
            if layout.follows_content() {
                panel.content = measure_panel_content(view, layout, text)?;
            }
        }
        Ok(())
    }

    pub fn update(&mut self, surface: SurfaceId) -> Option<KeyboardMode> {
        self.records.get_mut(&surface).and_then(Panel::update)
    }

    pub fn update_transaction(&mut self, surface: SurfaceId, content: ContentState) -> PanelUpdate {
        let Some(panel) = self.records.get_mut(&surface) else {
            return PanelUpdate {
                keyboard: None,
                transaction: Transaction::Settled,
            };
        };
        let keyboard = panel.update();
        let requested = panel.requested();
        let transaction = if !panel.pending() {
            Transaction::Settled
        } else if panel.committing == Some(requested) {
            Transaction::Committing
        } else if content == ContentState::Current {
            panel.committing = Some(requested);
            Transaction::Committed
        } else {
            Transaction::NeedsFrame
        };
        PanelUpdate {
            keyboard,
            transaction,
        }
    }

    #[must_use]
    pub fn geometry_change(&self, surface: SurfaceId) -> Option<SurfaceChange> {
        (self.records.get(&surface)).map(|panel| SurfaceChange::Panel(panel.geometry))
    }

    #[must_use]
    pub fn requests(&self, surface: SurfaceId, state: &SurfaceState) -> Option<PanelRequests> {
        let geometry = match state.get(SurfaceField::Panel) {
            Some(SurfaceChange::Panel(geometry)) => Some(*geometry),
            _ => None,
        };
        (self.records.get(&surface)).map(|panel| PanelRequests {
            geometry,
            keyboard: panel.keyboard_mode,
        })
    }

    pub fn committed(&mut self, surface: SurfaceId, requests: PanelRequests) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.apply(requests);
        }
    }

    pub fn shown(&mut self, surface: SurfaceId, content: PanelContent, requests: PanelRequests) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.apply(requests);
            panel.shown = Some(content);
        }
    }
}

fn measure_panel_content(
    view: &mut View,
    layout: LayerLayout,
    text: &mut impl TextMeasurer,
) -> Result<ContentSize, ViewError> {
    let size = measure_content(view, layout.content_proposal(), text)?;
    Ok(ContentSize::try_from(size).map_err(Error::Content)?)
}

#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
