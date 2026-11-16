// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::transfer::TransferError;
use lavashell::prelude::NumberError;
use lavashell_example_support::{ExampleError, NextStep};
use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("no home directory to keep the sample files in")]
    NoHome,
    #[error("cannot create the cache directory {path}")]
    CreateCache {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot create the directory of the sample files")]
    CreateSamples(#[source] io::Error),
    #[error("cannot write the sample file {path}")]
    WriteSample {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot resolve the sample file {path}")]
    ResolveSample {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the sample file {0} has no file URI")]
    FileUri(PathBuf),
    #[error("cannot offer the sample")]
    Offer(#[source] TransferError),
    #[error("the cut text changed before the move ended and stays")]
    CutTextChanged,
    #[error("cannot read the size of the image")]
    ReadImage(#[source] lavashell::renderer::Error),
    #[error("cannot find the executable of the example")]
    CurrentExecutable(#[source] io::Error),
    #[error("cannot start the shaped window")]
    StartChild(#[source] io::Error),
    #[error("cannot watch the process of the shaped window")]
    WatchChild(#[source] rustix::io::Errno),
    #[error("cannot count the corners of the round hole of the shaped window")]
    HoleCorners(#[source] NumberError),
}

impl ExampleError for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::NoHome => NextStep::SetHome,
            Self::CreateCache { .. } | Self::CreateSamples(_) | Self::WriteSample { .. } => {
                NextStep::MakeCacheWritable
            }
            Self::Setup(_)
            | Self::ResolveSample { .. }
            | Self::FileUri(_)
            | Self::Offer(_)
            | Self::CutTextChanged
            | Self::ReadImage(_)
            | Self::CurrentExecutable(_)
            | Self::StartChild(_)
            | Self::WatchChild(_)
            | Self::HoleCorners(_) => NextStep::ReportDefect,
        }
    }
}
