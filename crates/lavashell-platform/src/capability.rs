// SPDX-License-Identifier: GPL-3.0-only
use lavashell_enum_values::enum_with_values;
use std::fmt;

macro_rules! capabilities {
    (pub enum $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        enum_with_values! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
            pub enum $name {
                $($variant),+
            }

            pub const ALL;
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(match self {
                    $(Self::$variant => $text),+
                })
            }
        }
    };
}

capabilities! {
    pub enum Capability {
        Panels => "panels",
        SessionLock => "session lock",
        FocusGrab => "focus grabs",
        Clipboard => "clipboard",
        DragAndDrop => "drag and drop",
        Accessibility => "accessibility",
    }
}
