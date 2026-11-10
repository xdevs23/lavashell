// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;

/// An optional subsystem of the runtime, offered only by some platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// Panels placed by the host at the edges of an output.
    Panels,
    /// A session lock with a lock surface on every output.
    SessionLock,
    /// Focus grabs, which limit the input focus to a panel while the grab is active.
    FocusGrab,
    /// Window moves by the host, started from the drag region of a window.
    WindowMove,
    /// The clipboard selection shared with other clients.
    Clipboard,
    /// Drags offered to the app by other clients, and drags started by the app.
    DragAndDrop,
    /// The accessibility trees of the surfaces, published for assistive technologies.
    Accessibility,
}

impl Capability {
    pub const ALL: [Self; 7] = [
        Self::Panels,
        Self::SessionLock,
        Self::FocusGrab,
        Self::WindowMove,
        Self::Clipboard,
        Self::DragAndDrop,
        Self::Accessibility,
    ];
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Panels => "panels",
            Self::SessionLock => "session lock",
            Self::FocusGrab => "focus grabs",
            Self::WindowMove => "window moves",
            Self::Clipboard => "clipboard",
            Self::DragAndDrop => "drag and drop",
            Self::Accessibility => "accessibility",
        })
    }
}

#[cfg(test)]
mod tests;
