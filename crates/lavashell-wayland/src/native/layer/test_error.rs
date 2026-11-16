// SPDX-License-Identifier: GPL-3.0-only
#[derive(Debug, thiserror::Error)]
pub(crate) enum LayerTestError {
    #[error("the wire has no layer")]
    NoLayer,
}
