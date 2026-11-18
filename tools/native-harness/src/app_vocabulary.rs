// SPDX-License-Identifier: GPL-3.0-only
use lavashell_enum_values::enum_with_values;
use serde::Deserialize;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Host {
        Aosp,
        Checkout,
        Wayland,
    }

    const ALL;
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum LaunchKind {
        Helper,
        Window,
    }

    const ALL;
}

#[cfg(test)]
mod tests;
