// SPDX-License-Identifier: GPL-3.0-only
use crate::surface_records::measure_content;
use crate::view_actor::ViewError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins, PanelGeometry,
};
use lavashell_core::reactive::State;
use lavashell_core::surface::{SurfaceChange, SurfaceField, SurfaceId, SurfaceState};
use lavashell_core::view::{TextMeasurer, View};
use lavashell_platform::KeyboardMode;
use std::collections::BTreeMap;
use std::rc::Rc;

mod error;
mod per_output;

pub use error::Error;
pub use per_output::PanelsPerOutput;

/// When a panel receives the keyboard focus.
#[derive(Clone)]
pub enum KeyboardPolicy {
    Off,
    OnDemand,
    /// On demand, while the condition returns true.
    OnDemandWhen(Rc<dyn Fn() -> bool>),
}

impl KeyboardPolicy {
    /// The keyboard mode that the policy asks for at this moment.
    #[must_use]
    pub fn mode(&self) -> KeyboardMode {
        match self {
            Self::OnDemand => KeyboardMode::OnDemand,
            Self::OnDemandWhen(active) if active() => KeyboardMode::OnDemand,
            Self::Off | Self::OnDemandWhen(_) => KeyboardMode::Off,
        }
    }
}

/// How a panel lays out on its output and when it receives the keyboard focus.
#[derive(Clone)]
pub struct PanelOptions {
    /// The layout while no layout state is set.
    pub layout: LayerLayout,
    /// The reactive state that contains the layout of the panel. A change of the state is sent to the host with the
    /// next frame of the panel.
    pub layout_state: Option<State<LayerLayout>>,
    pub keyboard: KeyboardPolicy,
}

impl Default for PanelOptions {
    /// A bar across the top edge of the output, as tall as its content, which reserves its height and does not
    /// receive the keyboard focus.
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
    /// The value of the layout state, or `layout` without a layout state.
    #[must_use]
    pub fn current_layout(&self) -> LayerLayout {
        (self.layout_state.as_ref()).map_or(self.layout, State::get)
    }
}

/// What a frame shown by the host contains: the size of the content and whether the content accepts input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelContent {
    pub size: Size,
    pub input: bool,
}

/// Whether the host shows the current content of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentState {
    Current,
    /// The host shows content that is older than the view of the panel.
    Stale,
}

/// What follows from the requests of an update of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Transaction {
    /// No request waits for a commit.
    Settled,
    /// The platform commits the requests and the geometry at once, without a new frame, and reports the commit with
    /// [`PanelRecords::committed`].
    Committed,
    /// A commit without a frame of the requests and the geometry runs already.
    Committing,
    /// The requests and the geometry wait for the next frame of the panel, which commits them with its buffer.
    NeedsFrame,
}

/// What an update of a panel asks the platform to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct PanelUpdate {
    /// The keyboard mode to send, once the mode changed.
    pub keyboard: Option<KeyboardMode>,
    pub transaction: Transaction,
}

/// The geometry and the keyboard mode that one commit of a panel applies on the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelRequests {
    geometry: Option<PanelGeometry>,
    keyboard: KeyboardMode,
}

/// A panel on the host: its geometry, the keyboard mode sent last, the requests that the host applied last, and the
/// content of the last frame shown.
pub struct Panel {
    options: PanelOptions,
    content: ContentSize,
    geometry: PanelGeometry,
    keyboard_mode: KeyboardMode,
    applied: Option<PanelRequests>,
    /// The requests of the commit without a frame that the platform runs, until a commit or a frame applies them.
    committing: Option<PanelRequests>,
    shown: Option<PanelContent>,
}

impl Panel {
    /// A panel with `options` that shows `view`. The content of `view` is measured within the fixed lengths of the
    /// layout.
    ///
    /// # Errors
    ///
    /// Fails when the view cannot be measured or measures a length that is negative or not finite.
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
            // The first commit of a panel only asks the host for a configure event. The geometry and the keyboard
            // mode apply with the first content.
            applied: None,
            committing: None,
            shown: None,
            content,
            options,
        })
    }

    /// The geometry that the surface state of the panel sends to the host.
    #[must_use]
    pub fn geometry(&self) -> PanelGeometry {
        self.geometry
    }

    /// The keyboard mode sent to the host last.
    #[must_use]
    pub fn keyboard_mode(&self) -> KeyboardMode {
        self.keyboard_mode
    }

    /// Whether the keyboard policy of the panel allows the keyboard focus at this moment.
    #[must_use]
    pub fn keyboard_allowed(&self) -> bool {
        self.options.keyboard.mode() == KeyboardMode::OnDemand
    }

    /// Whether the host gives the keyboard focus to the panel: the policy allows it, and the host applied the mode
    /// that allows it with no request waiting for a commit.
    #[must_use]
    pub fn keyboard_granted(&self) -> bool {
        !self.pending() && self.keyboard_mode == KeyboardMode::OnDemand && self.keyboard_allowed()
    }

    /// Whether the content of the last frame shown accepts input.
    #[must_use]
    pub fn input_enabled(&self) -> bool {
        self.shown.is_some_and(|content| content.input)
    }

    /// Whether the content of the last frame shown has `size`.
    #[must_use]
    pub fn content_current(&self, size: Size) -> bool {
        self.shown.is_some_and(|content| content.size == size)
    }

    /// Lays the panel out again, and returns the keyboard mode to send once the mode changed.
    fn update(&mut self) -> Option<KeyboardMode> {
        self.geometry = PanelGeometry::of(self.options.current_layout(), self.content);
        let keyboard = self.options.keyboard.mode();
        if keyboard == self.keyboard_mode {
            return None;
        }
        self.keyboard_mode = keyboard;
        Some(keyboard)
    }

    /// The geometry and the keyboard mode sent to the host last.
    fn requested(&self) -> PanelRequests {
        PanelRequests {
            geometry: Some(self.geometry),
            keyboard: self.keyboard_mode,
        }
    }

    /// Whether a request for the geometry or the keyboard mode waits for a commit.
    fn pending(&self) -> bool {
        self.applied != Some(self.requested())
    }

    /// Records that the host applied `requests`, which ends the commit without a frame of the same requests.
    fn apply(&mut self, requests: PanelRequests) {
        if self.committing == Some(requests) {
            self.committing = None;
        }
        self.applied = Some(requests);
    }
}

/// Every open panel, by the id of its surface.
#[derive(Default)]
pub struct PanelRecords {
    records: BTreeMap<SurfaceId, Panel>,
}

impl PanelRecords {
    pub fn open(&mut self, surface: SurfaceId, panel: Panel) {
        let _replaced_panel = self.records.insert(surface, panel);
    }

    /// Removes the panel shown on `surface` after the surface has retired.
    pub fn forget(&mut self, surface: SurfaceId) {
        let _removed_panel = self.records.remove(&surface);
    }

    #[must_use]
    pub fn get(&self, surface: SurfaceId) -> Option<&Panel> {
        self.records.get(&surface)
    }

    /// Measures the content of `view` again when a length of the panel of `surface` follows the content. The next
    /// update lays the panel out for that content.
    ///
    /// # Errors
    ///
    /// Fails when the view cannot be measured or measures a length that is negative or not finite.
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

    /// Lays out the panel of `surface` again, and returns the keyboard mode to send once the mode changed. The next
    /// frame of the panel commits the geometry and the keyboard mode with its buffer.
    pub fn update(&mut self, surface: SurfaceId) -> Option<KeyboardMode> {
        self.records.get_mut(&surface).and_then(Panel::update)
    }

    /// Runs `update`, and returns what follows for the geometry and the keyboard mode of the panel. While the host
    /// shows the current content of the panel, the platform commits them without a frame.
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

    /// The change of the surface state that sets the geometry of the panel of `surface`, or `None` when the surface
    /// has no panel.
    #[must_use]
    pub fn geometry_change(&self, surface: SurfaceId) -> Option<SurfaceChange> {
        (self.records.get(&surface)).map(|panel| SurfaceChange::Panel(panel.geometry))
    }

    /// The requests that a commit of `surface` sent from now on applies with `state`: the keyboard mode sent last,
    /// and the geometry in `state`. Returns `None` when the surface has no panel.
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

    /// Records that the host applied `requests` to the panel of `surface` with a commit without a frame.
    pub fn committed(&mut self, surface: SurfaceId, requests: PanelRequests) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.apply(requests);
        }
    }

    /// Records that the host shows a frame of the panel of `surface` with `content`, which applied `requests`.
    pub fn shown(&mut self, surface: SurfaceId, content: PanelContent, requests: PanelRequests) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.apply(requests);
            panel.shown = Some(content);
        }
    }
}

/// The size of the content of `view`, measured within the fixed lengths of `layout`.
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
