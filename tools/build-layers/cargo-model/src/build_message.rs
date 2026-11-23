// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages
use crate::package_id::PackageId;
use crate::unit_graph::{DebugInfo, RuntimeChecks, TargetIdentity};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct ArtifactProfile {
    pub opt_level: String,
    pub debuginfo: Option<DebugInfo>,
    #[serde(flatten)]
    pub runtime_checks: RuntimeChecks,
    pub test: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct CompiledUnit {
    pub package_id: PackageId,
    pub target: TargetIdentity,
    pub profile: ArtifactProfile,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CompilerArtifact {
    #[serde(flatten)]
    pub unit: CompiledUnit,
    filenames: Vec<PathBuf>,
    pub fresh: bool,
}

impl CompilerArtifact {
    pub fn output_names(&self) -> impl Iterator<Item = PathBuf> {
        self.filenames
            .iter()
            .map(|file| file_name_with_directory(file))
    }
}

fn file_name_with_directory(file: &Path) -> PathBuf {
    let components: Vec<Component<'_>> = file.components().collect();
    components[components.len().saturating_sub(2)..]
        .iter()
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum BuildMessage {
    CompilerArtifact(Box<CompilerArtifact>),
    #[serde(other)]
    Other,
}
