// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Surfaces {
    Window,
    Layer,
    Both,
}

impl Surfaces {
    pub(crate) fn has_window(self) -> bool {
        matches!(self, Self::Window | Self::Both)
    }

    pub(crate) fn has_layer(self) -> bool {
        matches!(self, Self::Layer | Self::Both)
    }
}
