// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PlatformRequest;
use std::sync::mpsc;

/// A failure of the test platform.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the test platform connects only inside `HostScript::serve`, on the thread that called it"
    )]
    NoScript,
    #[error("no test platform connected inside `HostScript::serve`")]
    NotServed(#[source] mpsc::RecvError),
    #[error("the test platform cannot carry out the request {0:?}")]
    Unsupported(Box<PlatformRequest>),
    #[error("no open window has the app id {app_id}")]
    NoWindow { app_id: String },
    #[error("cannot encode the frame shown by surface {} as PNG", surface.0)]
    EncodeFrame {
        surface: SurfaceId,
        #[source]
        source: lavashell_renderer::Error,
    },
}
