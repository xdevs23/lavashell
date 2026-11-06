// SPDX-License-Identifier: GPL-3.0-only

/// When a panel receives the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    /// The panel never has the keyboard focus.
    Off,
    /// The panel has the keyboard focus once the person moves the focus to it, such as by clicking into it.
    OnDemand,
}
