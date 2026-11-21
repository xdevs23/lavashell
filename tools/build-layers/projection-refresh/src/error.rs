// SPDX-License-Identifier: GPL-3.0-only
use lavashell_cargo_model::dependency_kind::DependencyKind;
use lavashell_cargo_model::package_id::PackageId;
use lavashell_cargo_model::unit_graph::{TargetKind, UnitIndexError, UnitMode};
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
    #[error("{path} cannot be written as TOML: {source}")]
    WriteToml {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },
    #[error(transparent)]
    Cargo(CommandError),
    #[error(transparent)]
    Nix(#[from] NixError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("Cargo reported the invalid unit index {}", .0.index)]
    UnitIndex(#[from] UnitIndexError),
    #[error(
        "the canonical dependency plan lost {missing} external contexts and added {extra} others"
    )]
    ExternalContextsChanged { missing: usize, extra: usize },
    #[error(
        "the path package {package} is outside the workspace and cannot be replaced by a \
         workspace stub"
    )]
    ExternalPathPackage { package: PackageId },
    #[error(
        "the member {member} is not a contained path of a virtual workspace, which dependency \
         projections need"
    )]
    NonVirtualWorkspace { member: String },
    #[error(
        "the dependency edge {parent} -> {child} sets flags that a dependency layer cannot \
         reproduce"
    )]
    UnsupportedEdgeFlags { parent: String, child: String },
    #[error(
        "the declared source {declared} is not supported, because registry and git \
         configuration are not inferred"
    )]
    UnsupportedSource { declared: String },
    #[error("the dependency of {package} on {dependency} declares no source")]
    UndeclaredSource { package: String, dependency: String },
    #[error("the selected engine member {member} has an original links contract")]
    OriginalLinks { member: String },
    #[error(
        "the selected engine member {member} has an original build script that its seed did \
         not follow"
    )]
    OriginalBuildScript { member: String },
    #[error("path patches and replacements need real source handling, not dummy dependency stubs")]
    PathPatch,
    #[error("external units depend on local source, and a dummy layer cannot preserve that input")]
    ExternalDependsOnLocal,
    #[error("the local package {package} links native code, which an empty stub cannot replace")]
    LocalLinks { package: String },
    #[error("a dependency layer does not support the Cargo unit mode {mode:?}")]
    UnsupportedMode { mode: UnitMode },
    #[error("a dependency layer does not support the local target kinds {kinds:?}")]
    UnsupportedTargetKind { kinds: Vec<TargetKind> },
    #[error("Cargo selected no units")]
    NoUnitsSelected,
    #[error("Cargo metadata and the unit graph disagree on the package {package}")]
    PackageIdentityMismatch { package: PackageId },
    #[error("several resolved packages occupy one workspace member")]
    SharedMember,
    #[error("the dependency alias of {crate_name} in {package} is ambiguous or missing")]
    AmbiguousAlias { crate_name: String, package: String },
    #[error(
        "distinct host and target requests share {package}.{kind}.{alias}, and one manifest \
         cannot merge them"
    )]
    ConflictingRequests {
        package: String,
        kind: DependencyKind,
        alias: String,
    },
    #[error("the selected root package is missing from the active local closure")]
    RootPackageMissing,
    #[error("the feature context of {package} is ambiguous")]
    AmbiguousFeatureContext { package: PackageId },
    #[error("the package id {package} names no package and version")]
    UnnamedPackage { package: PackageId },
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
    ModelInput,
    FixCargo,
    FixNix,
    Discard,
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
            Self::ExternalContextsChanged { .. } => NextStep::Discard,
            Self::ExternalPathPackage { .. }
            | Self::NonVirtualWorkspace { .. }
            | Self::UnsupportedEdgeFlags { .. }
            | Self::UnsupportedSource { .. }
            | Self::UndeclaredSource { .. }
            | Self::OriginalLinks { .. }
            | Self::OriginalBuildScript { .. }
            | Self::PathPatch
            | Self::ExternalDependsOnLocal
            | Self::LocalLinks { .. }
            | Self::UnsupportedMode { .. }
            | Self::UnsupportedTargetKind { .. } => NextStep::ModelInput,
            Self::UnitIndex(_)
            | Self::NoUnitsSelected
            | Self::PackageIdentityMismatch { .. }
            | Self::SharedMember
            | Self::AmbiguousAlias { .. }
            | Self::ConflictingRequests { .. }
            | Self::RootPackageMissing
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
            | Self::WriteToml { .. }
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
            Self::ModelInput => concat!(
                "Remove it from the crates built in layers, ",
                "or extend the projection refresh in tools/build-layers to model it.",
            ),
            Self::FixCargo => "Fix the Cargo error, then run the refresh again.",
            Self::FixNix => "Fix the Nix evaluation error, then run the refresh again.",
            Self::Discard => {
                "Do not publish this projection. Check the Cargo output of the refresh."
            }
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
