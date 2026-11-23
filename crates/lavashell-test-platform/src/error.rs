// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::keyboard::Key;
use lavashell_core::surface::SurfaceId;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
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
    #[error("the test platform has not opened surface {} as a window", surface.get())]
    NotAWindow { surface: SurfaceId },
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

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::ReadFrame { source, .. } | Self::EncodeFrame { source, .. } => source.next_step(),
            Self::NotServed(_)
            | Self::Unsupported(_)
            | Self::Press { .. }
            | Self::NoWindow { .. }
            | Self::NotAWindow { .. }
            | Self::NoRest { .. } => NextStep::ReportTheFailure,
        }
    }
}
