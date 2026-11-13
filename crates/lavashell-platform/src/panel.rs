// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

/// What the runtime asks a platform to do with a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelRequest {
    /// Sets when the panel on the surface receives the keyboard focus.
    SetKeyboard(SurfaceId, KeyboardMode),
}

/// When a panel receives the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    /// The panel never has the keyboard focus.
    Off,
    /// The panel has the keyboard focus once the person moves the focus to it, such as by clicking into it.
    OnDemand,
}
