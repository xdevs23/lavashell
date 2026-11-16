// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PlatformRequest;
use std::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no test platform connected inside `HostScript::serve`")]
    NotServed(#[source] mpsc::RecvError),
    #[error("the test platform cannot carry out the request {0:?}")]
    Unsupported(Box<PlatformRequest>),
    #[error("no open window has the app id {app_id}")]
    NoWindow { app_id: String },
    #[error("cannot encode the frame shown by surface {} as PNG", surface.get())]
    EncodeFrame {
        surface: SurfaceId,
        #[source]
        source: lavashell_renderer::Error,
    },
}
