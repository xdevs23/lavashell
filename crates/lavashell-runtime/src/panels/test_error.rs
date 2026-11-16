// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, thiserror::Error)]
pub(crate) enum PanelsTestError {
    #[error("the surface has no panel")]
    NoPanel,
}
