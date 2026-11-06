// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::panel::PanelGeometry;
use lavashell_core::surface::SurfaceId;

/// When a panel receives the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    /// The panel never has the keyboard focus.
    Off,
    /// The panel has the keyboard focus once the person moves the focus to it, such as by clicking into it.
    OnDemand,
}

/// What the runtime asks a platform to do with a panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelRequest {
    /// Sets the geometry of the panel. The host applies it with the next commit of the panel.
    Update(SurfaceId, PanelGeometry),
    /// Sets when the panel receives the keyboard focus. The host applies it with the next commit of the panel.
    SetKeyboard(SurfaceId, KeyboardMode),
    /// Commits the geometry and the keyboard mode set since the last commit, without a new frame.
    Commit(SurfaceId),
}
