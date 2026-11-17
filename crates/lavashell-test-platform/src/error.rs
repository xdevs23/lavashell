// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::keyboard::Key;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PlatformRequest;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no test platform connected inside `HostScript::serve`")]
    NotServed(#[source] mpsc::RecvError),
    #[error("the test platform cannot carry out the request {0:?}")]
    Unsupported(Box<PlatformRequest>),
    #[error("the test platform has no Linux key code to press {key:?}")]
    Press { key: Key },
    #[error("no open window has the app id {app_id}")]
    NoWindow { app_id: String },
    #[error("frames keep arriving at {time:?} on the virtual clock, so the run never finishes")]
    NoRest { time: Duration },
    #[error("cannot read the pixels of the frame painted by surface {}", surface.get())]
    ReadFrame {
        surface: SurfaceId,
        #[source]
        source: lavashell_renderer::Error,
    },
    #[error("cannot encode the frame shown by surface {} as PNG", surface.get())]
    EncodeFrame {
        surface: SurfaceId,
        #[source]
        source: lavashell_renderer::Error,
    },
}
