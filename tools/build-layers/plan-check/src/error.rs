// SPDX-License-Identifier: GPL-3.0-only
use lavashell_plan_check::stub_layer_reuse::{RecompileCause, StubLayerRecordError};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path} cannot be opened: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} cannot be read: {source}")]
    ReadLine {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    StubLayerRecord(#[from] StubLayerRecordError),
}

pub enum NextStep {
    PlanUnitInStubLayer,
    BuildOnStubLayerResolvingLikePackage,
    BuildLikeStubLayer,
    ReportDefect,
}

impl Error {
    pub fn next_step(&self) -> NextStep {
        match self {
            Self::Open { .. }
            | Self::Read { .. }
            | Self::ReadLine { .. }
            | Self::StubLayerRecord(_) => NextStep::ReportDefect,
        }
    }
}

impl From<RecompileCause> for NextStep {
    fn from(cause: RecompileCause) -> Self {
        match cause {
            RecompileCause::UnplannedVariant => Self::PlanUnitInStubLayer,
            RecompileCause::DependenciesResolvedDifferently => {
                Self::BuildOnStubLayerResolvingLikePackage
            }
            RecompileCause::PlannedUnitRebuilt => Self::BuildLikeStubLayer,
        }
    }
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlanUnitInStubLayer => formatter.write_str(
                "Extend the projection refresh in tools/build-layers so that the stub layer plans this unit. \
                 Run `nix run .#refresh-cargo-projections` and commit the refreshed projections.",
            ),
            Self::BuildOnStubLayerResolvingLikePackage => formatter.write_str(
                "This package does not share the dependency resolution of the stub layer. \
                 Build it above a stub layer that resolves its dependencies like it does, \
                 such as a registry layer of its own, or change the route that puts it on this stub layer.",
            ),
            Self::BuildLikeStubLayer => formatter.write_str(
                "Give this build the environment, flags and build-script inputs that the stub layer built with, \
                 so that Cargo reuses the unit.",
            ),
            Self::ReportDefect => formatter.write_str(
                "Cargo or Nix wrote output that the plan check cannot read. Report this output to the developers.",
            ),
        }
    }
}
