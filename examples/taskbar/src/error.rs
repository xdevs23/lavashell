// SPDX-License-Identifier: GPL-3.0-only
use crate::model::App;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("the scene has no taskbar")]
    MissingBar,
    #[error("the taskbar has no slot for {}", .0.name())]
    MissingSlot(App),
}
