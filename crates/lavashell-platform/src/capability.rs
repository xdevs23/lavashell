// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;

macro_rules! capabilities {
    ($($variant:ident => $text:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Capability {
            $($variant),*
        }

        impl Capability {
            pub const ALL: [Self; [$(Self::$variant),*].len()] = [$(Self::$variant),*];
        }

        impl fmt::Display for Capability {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(match self {
                    $(Self::$variant => $text),*
                })
            }
        }
    };
}

capabilities! {
    Panels => "panels",
    SessionLock => "session lock",
    FocusGrab => "focus grabs",
    WindowMove => "window moves",
    Clipboard => "clipboard",
    DragAndDrop => "drag and drop",
    Accessibility => "accessibility",
}
