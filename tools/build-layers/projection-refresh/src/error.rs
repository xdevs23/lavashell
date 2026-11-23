// SPDX-License-Identifier: GPL-3.0-only
use lavashell_cargo_model::package_id::PackageId;
use lavashell_cargo_model::unit_graph::UnitIndexError;
use lavashell_command_output::json::CommandError;
use lavashell_command_output::nix::NixError;
use lavashell_plan_check::PlanError;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{directory} is not the root of a Lavashell checkout")]
    NotRepositoryRoot { directory: PathBuf },
    #[error("{path} cannot be read: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} cannot be written: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the directory {path} cannot be listed: {source}")]
    ListDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the refreshed projection cannot replace {path}: {source}")]
    PublishProjection {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the temporary directory of the refresh cannot be created: {0}")]
    TemporaryDirectory(#[source] std::io::Error),
    #[error("{path} is not the expected JSON document: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} is not the expected TOML document: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{path} cannot be written as JSON: {source}")]
    WriteJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Cargo(CommandError),
    #[error(transparent)]
    Nix(#[from] NixError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("Cargo reported the invalid unit index {}", .0.index)]
    UnitIndex(#[from] UnitIndexError),
    #[error("the feature context of {package} is ambiguous")]
    AmbiguousFeatureContext { package: Box<PackageId> },
    #[error("the package id {package} names no package and version")]
    UnnamedPackage { package: Box<PackageId> },
    #[error("the shared features of {package} did not change in the last round")]
    SharedFeaturesUnchanged { package: String },
    #[error(
        "these packages are neither examples, packaged crates nor the native base: {}",
        .packages.join(", ")
    )]
    UnknownPackages { packages: Vec<String> },
    #[error("while refreshing the projection of {package}: {source}")]
    Refresh {
        package: String,
        #[source]
        source: Box<Error>,
    },
    #[error("a refresh thread ended without a result")]
    RefreshThread,
    #[error("the layer catalog of the Nix sources does not list {directory}")]
    CatalogIncomplete { directory: String },
}

pub enum NextStep {
    RefreshProjections,
    EnterShell,
    RunFromRoot,
    CheckManifests,
    FixCargo,
    FixNix,
    ChooseKnownPackages,
    RefreshTheRest,
    ReportDefect,
}

impl Error {
    pub fn next_step(&self) -> NextStep {
        match self {
            Self::Refresh { source, .. } => source.next_step(),
            Self::Nix(NixError::NixpkgsUnset) => NextStep::EnterShell,
            Self::NotRepositoryRoot { .. } => NextStep::RunFromRoot,
            Self::Plan(_) => NextStep::RefreshProjections,
            Self::Cargo(CommandError::Failed { .. }) => NextStep::FixCargo,
            Self::Nix(NixError::Command(CommandError::Failed { .. })) => NextStep::FixNix,
            Self::UnitIndex(_)
            | Self::AmbiguousFeatureContext { .. }
            | Self::SharedFeaturesUnchanged { .. } => NextStep::CheckManifests,
            Self::UnknownPackages { .. } => NextStep::ChooseKnownPackages,
            Self::PublishProjection { .. } => NextStep::RefreshTheRest,
            Self::ReadFile { .. }
            | Self::WriteFile { .. }
            | Self::ListDirectory { .. }
            | Self::TemporaryDirectory(_)
            | Self::ParseJson { .. }
            | Self::ParseToml { .. }
            | Self::WriteJson { .. }
            | Self::Cargo(_)
            | Self::Nix(_)
            | Self::UnnamedPackage { .. }
            | Self::RefreshThread
            | Self::CatalogIncomplete { .. } => NextStep::ReportDefect,
        }
    }
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RefreshProjections => {
                "Run `nix run .#refresh-cargo-projections` and commit the refreshed projections."
            }
            Self::EnterShell => concat!(
                "Run the refresh through `nix run .#refresh-cargo-projections`, ",
                "which provides cargo, rustc and Nix.",
            ),
            Self::RunFromRoot => "Run the refresh from the root of the checkout.",
            Self::CheckManifests => {
                "Check the Cargo manifests of the selected packages, then run the refresh again."
            }
            Self::FixCargo => "Fix the Cargo error, then run the refresh again.",
            Self::FixNix => "Fix the Nix evaluation error, then run the refresh again.",
            Self::ChooseKnownPackages => {
                "Name only examples, packaged crates of the projection contract or the native base."
            }
            Self::RefreshTheRest => concat!(
                "Part of the projections in the checkout are refreshed and the rest are not. ",
                "Fix the error, then run the refresh again to refresh the rest.",
            ),
            Self::ReportDefect => {
                "The refresh cannot read or write its files. Report this output to the developers."
            }
        })
    }
}
