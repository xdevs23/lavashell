// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, measure_content};
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins, PanelGeometry,
};
use lavashell_core::reactive::State;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::{TextMeasurer, View};
use lavashell_platform::{KeyboardMode, PanelRequest};
use std::collections::BTreeMap;
use std::rc::Rc;

mod per_output;

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
    /// The requests end with a commit without a new frame.
    Committed,
    /// The requests wait for the next frame of the panel, which commits them with its buffer.
    NeedsFrame,
}

/// A panel on the host: the geometry and the keyboard mode sent last, whether they wait for a commit, and the
/// content of the last frame shown.
pub struct Panel {
    options: PanelOptions,
    content: ContentSize,
    geometry: PanelGeometry,
    keyboard_mode: KeyboardMode,
    committed_keyboard: KeyboardMode,
    shown: Option<PanelContent>,
    pending: bool,
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
    ) -> Result<Self, Error> {
        let layout = options.current_layout();
        let content = measure_panel_content(view, layout, text)?;
        Ok(Self {
            geometry: PanelGeometry::of(layout, content),
            keyboard_mode: options.keyboard.mode(),
            // The first commit of a panel only asks the host for a configure event. The keyboard mode is committed
            // with the first content.
            committed_keyboard: KeyboardMode::Off,
            shown: None,
            pending: true,
            content,
            options,
        })
    }

    /// The geometry sent to the host last.
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
        !self.pending
            && self.committed_keyboard == KeyboardMode::OnDemand
            && self.keyboard_allowed()
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

    fn update(&mut self, surface: SurfaceId, out: &mut Vec<PanelRequest>) {
        let geometry = PanelGeometry::of(self.options.current_layout(), self.content);
        if geometry != self.geometry {
            out.push(PanelRequest::Update(surface, geometry));
            self.geometry = geometry;
            self.pending = true;
        }
        let keyboard = self.options.keyboard.mode();
        if keyboard != self.keyboard_mode {
            out.push(PanelRequest::SetKeyboard(surface, keyboard));
            self.keyboard_mode = keyboard;
            self.pending = true;
        }
    }

    fn submitted(&mut self) {
        self.committed_keyboard = self.keyboard_mode;
        self.pending = false;
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
    ) -> Result<(), Error> {
        if let Some(panel) = self.records.get_mut(&surface) {
            let layout = panel.options.current_layout();
            if layout.follows_content() {
                panel.content = measure_panel_content(view, layout, text)?;
            }
        }
        Ok(())
    }

    /// Adds to `out` the requests for the layout and the keyboard mode of the panel of `surface`, once either
    /// changed. The next frame of the panel commits them with its buffer.
    pub fn update(&mut self, surface: SurfaceId, out: &mut Vec<PanelRequest>) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.update(surface, out);
        }
    }

    /// Adds to `out` the requests of `update`, and a commit of every request waiting for one while the host shows
    /// the current content of the panel.
    pub fn update_transaction(
        &mut self,
        surface: SurfaceId,
        content: ContentState,
        out: &mut Vec<PanelRequest>,
    ) -> Transaction {
        let Some(panel) = self.records.get_mut(&surface) else {
            return Transaction::Settled;
        };
        panel.update(surface, out);
        match (panel.pending, content) {
            (false, _) => Transaction::Settled,
            (true, ContentState::Current) => {
                out.push(PanelRequest::Commit(surface));
                panel.submitted();
                Transaction::Committed
            }
            (true, ContentState::Stale) => Transaction::NeedsFrame,
        }
    }

    /// Records that the host shows a frame of the panel of `surface` with `content`. The frame committed every
    /// request made before it.
    pub fn shown(&mut self, surface: SurfaceId, content: PanelContent) {
        if let Some(panel) = self.records.get_mut(&surface) {
            panel.submitted();
            panel.shown = Some(content);
        }
    }
}

/// The size of the content of `view`, measured within the fixed lengths of `layout`.
fn measure_panel_content(
    view: &mut View,
    layout: LayerLayout,
    text: &mut impl TextMeasurer,
) -> Result<ContentSize, Error> {
    let size = measure_content(view, layout.content_proposal(), text)?;
    ContentSize::try_from(size).map_err(Error::PanelContent)
}

#[cfg(test)]
mod tests;
