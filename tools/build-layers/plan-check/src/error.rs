// SPDX-License-Identifier: GPL-3.0-only
use lavashell_plan_check::PlanError;
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
    #[error(transparent)]
    Plan(#[from] PlanError),
}

pub enum NextStep {
    RefreshProjections,
    ReportDefect,
}

impl Error {
    pub fn next_step(&self) -> NextStep {
        match self {
            Self::Plan(
                PlanError::EngineUnitsChanged { .. } | PlanError::EngineUnitsUncached { .. },
            ) => NextStep::RefreshProjections,
            Self::Open { .. } | Self::Read { .. } | Self::Plan(PlanError::UnitGraph(_)) => {
                NextStep::ReportDefect
            }
        }
    }
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RefreshProjections => {
                "Run `nix run .#refresh-cargo-projections` and commit the refreshed projections."
            }
            Self::ReportDefect => {
                "Cargo or Nix wrote output that the plan check cannot read. Report this output to the developers."
            }
        })
    }
}
