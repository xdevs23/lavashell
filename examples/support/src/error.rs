// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::scene::tree::ApplyError;
use lavashell::core::view::{ComponentFailure, FrameError};
use lavashell::prelude::{NumberError, Role};
use lavashell::renderer::Error as RendererError;
use std::{num::ParseFloatError, path::PathBuf, time::TryFromFloatSecsError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the example fonts need {} set to the directory of their dependencies",
        crate::FONT_DIRECTORY
    )]
    FontDirectory,
    #[error("cannot load the example fonts")]
    Fonts(#[source] RendererError),
    #[error("cannot create the headless Vulkan renderer")]
    Renderer(#[source] RendererError),
    #[error("cannot create the painter of the capture")]
    Painter(#[source] RendererError),
    #[error("no buffer of whole pixels covers the size of the capture")]
    BufferSize(#[source] NumberError),
    #[error("cannot create the offscreen surface of the capture")]
    Offscreen(#[source] RendererError),
    #[error("cannot encode the capture as PNG")]
    EncodePng(#[source] RendererError),
    #[error("cannot create the directory {}", path.display())]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write the capture {}", path.display())]
    WriteCapture {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot show the frame of the view")]
    Frame(#[source] FrameError),
    #[error("a component of the view failed in a frame")]
    ComponentsFailed(Vec<ComponentFailure>),
    #[error("cannot measure the content of the view")]
    Measure(#[source] NumberError),
    #[error("nothing is left that could move the view on")]
    NothingMoves,
    #[error("the view asks for another frame after a frame that changed nothing")]
    NoRest,
    #[error("cannot show the update of a frame in the scene of the capture")]
    ShowScene(#[source] ApplyError),
    #[error("the view has no control tagged {0}")]
    MissingControl(String),
    #[error("the view shows no {role} named {name}")]
    MissingNode { role: Role, name: String },
    #[cfg(target_os = "linux")]
    #[error("the app failed")]
    App(#[source] lavashell::wayland::Error),
    #[error("cannot create the frame log")]
    CreateFrameLog(#[source] csv::Error),
    #[error("cannot write a frame report to the frame log")]
    WriteFrameReport(#[source] csv::Error),
    #[error("cannot flush the frame log")]
    FlushFrameLog(#[source] std::io::Error),
    #[error("the value is not a number")]
    Number(#[source] ParseFloatError),
    #[error("a text scale must be finite and above zero")]
    TextScale,
    #[error("the value is not a duration in seconds")]
    Seconds(#[source] TryFromFloatSecsError),
    #[error("a length must be finite and at least zero")]
    Length(#[source] NumberError),
    #[error("a frame rate must be finite, above zero and at most one frame per nanosecond")]
    FrameRate,
    #[error("cannot decode an icon of the example")]
    Icon(#[source] lavashell_lucide::Error),
}
