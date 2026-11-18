// SPDX-License-Identifier: GPL-3.0-only
use super::Drawn;
use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::NumberError;
use std::convert::Infallible;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("the Vulkan renderer could not start")]
    CreateRenderer(#[source] Error),
    #[error("the painter could not start")]
    CreatePainter(#[source] Error),
    #[error("the GPU profiler could not be created")]
    CreateProfiler(#[source] Error),
    #[error("the Vulkan target of a surface could not be created")]
    CreateTarget(#[source] Error),
    #[error("the HDR metadata of a surface was rejected")]
    SetHdrMetadata(#[source] Error),
    #[error("the surface {0:?} is attached already")]
    AttachedTwice(SurfaceId),
    #[error("the surface {0:?} is not attached")]
    UnknownSurface(SurfaceId),
    #[error("no buffer of whole pixels covers the size of a surface")]
    BufferSize(#[source] NumberError),
    #[error("the contents of a drawn surface could not be checked")]
    ReadContents(#[source] Error),
    #[error("the scene images that the GPU is done with could not be released")]
    ReleaseSceneImports(#[source] Error),
    #[error("the completion of a scene could not be read")]
    SceneCompletion(#[source] Error),
    #[error("the scene source could not produce a scene")]
    ProduceScene(#[source] Error),
    #[error("the scene mailbox did not accept a produced scene")]
    PublishScene(#[source] Error),
}

#[derive(Debug, thiserror::Error)]
pub enum StartError<S> {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("the streams of the renderer could not start")]
    Streams(#[source] S),
}

impl StartError<Infallible> {
    pub(super) fn into_state(self) -> StateError {
        match self {
            Self::State(cause) => cause,
            Self::Streams(never) => match never {},
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError<C> {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("a frame could not be drawn")]
    Draw(#[source] Error),
    #[error("a frame could not be painted")]
    Paint(#[source] lavashell_renderer::Error),
    #[error("the platform could not prepare the present of a frame")]
    Control(#[source] C),
}

#[derive(Debug, thiserror::Error)]
pub enum DrawError<C, S = Infallible> {
    #[error(transparent)]
    Frame(#[from] FrameError<C>),
    #[error(
        "the frame was drawn, but the frames of the streams drawn by it could not be imported or returned"
    )]
    StreamsOfShownFrame {
        drawn: Drawn,
        #[source]
        streams: S,
    },
    #[error("the frame failed, and the frames of the streams drawn by it failed too: {streams}")]
    FrameAndStreams {
        #[source]
        frame: FrameError<C>,
        streams: S,
    },
}

impl<C, S> From<StateError> for DrawError<C, S> {
    fn from(failure: StateError) -> Self {
        Self::Frame(FrameError::State(failure))
    }
}

impl<C, S> DrawError<C, S> {
    pub fn combine(
        frame: Result<Drawn, FrameError<C>>,
        streams: Result<(), S>,
    ) -> Result<Drawn, Self> {
        match (frame, streams) {
            (frame, Ok(())) => frame.map_err(Self::Frame),
            (Ok(drawn), Err(streams)) => Err(Self::StreamsOfShownFrame { drawn, streams }),
            (Err(frame), Err(streams)) => Err(Self::FrameAndStreams { frame, streams }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommitError<E> {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("the platform could not commit the state of a surface")]
    Control(#[source] E),
}
