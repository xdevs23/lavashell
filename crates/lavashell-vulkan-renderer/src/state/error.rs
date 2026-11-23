// SPDX-License-Identifier: GPL-3.0-only
use super::Drawn;
use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::NumberError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
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

impl NamesNextStep for StateError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::CreateRenderer(error)
            | Self::CreatePainter(error)
            | Self::CreateProfiler(error)
            | Self::CreateTarget(error)
            | Self::SetHdrMetadata(error)
            | Self::ReadContents(error)
            | Self::ReleaseSceneImports(error)
            | Self::SceneCompletion(error)
            | Self::ProduceScene(error)
            | Self::PublishScene(error) => error.next_step(),
            Self::AttachedTwice(_) | Self::UnknownSurface(_) | Self::BufferSize(_) => {
                NextStep::ReportTheFailure
            }
        }
    }
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

impl<S: NamesNextStep> NamesNextStep for StartError<S> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::State(error) => error.next_step(),
            Self::Streams(error) => error.next_step(),
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

impl<C: NamesNextStep> NamesNextStep for FrameError<C> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::State(error) => error.next_step(),
            Self::Draw(error) => error.next_step(),
            Self::Paint(error) => error.next_step(),
            Self::Control(error) => error.next_step(),
        }
    }
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

impl<C: NamesNextStep, S: NamesNextStep> NamesNextStep for DrawError<C, S> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Frame(frame) | Self::FrameAndStreams { frame, .. } => frame.next_step(),
            Self::StreamsOfShownFrame { streams, .. } => streams.next_step(),
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

impl<E: NamesNextStep> NamesNextStep for CommitError<E> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::State(error) => error.next_step(),
            Self::Control(error) => error.next_step(),
        }
    }
}
