// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

/// The failures that can occur with focus grabs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the panel {0:?} has a focus grab, not supported by the platform. Run the app on a \
         platform that supports focus grabs, or remove the focus grab from the panel."
    )]
    Unsupported(SurfaceId),
}
