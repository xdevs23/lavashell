// SPDX-License-Identifier: GPL-3.0-only
use lavashell_enum_values::enum_with_values;
use std::fmt;

macro_rules! capabilities {
    ($($variant:ident => $text:literal),+ $(,)?) => {
        enum_with_values! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
            pub enum Capability {
                $($variant),+
            }

            pub const ALL;
        }

        impl fmt::Display for Capability {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(match self {
                    $(Self::$variant => $text),+
                })
            }
        }
    };
}

capabilities! {
    Panels => "panels",
    SessionLock => "session lock",
    FocusGrab => "focus grabs",
    Clipboard => "clipboard",
    DragAndDrop => "drag and drop",
    Accessibility => "accessibility",
}
