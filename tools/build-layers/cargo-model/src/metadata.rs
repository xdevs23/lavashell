// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/commands/cargo-metadata.html#json-format
use crate::dependency_kind::DependencyKind;
use crate::package_id::PackageId;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
pub struct CargoMetadata {
    pub workspace_root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Package {
    pub id: PackageId,
    pub name: String,
    pub version: String,
    pub edition: String,
    pub source: Option<String>,
    pub manifest_path: PathBuf,
    pub dependencies: Vec<PackageDependency>,
    pub links: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PackageDependency {
    pub name: String,
    pub rename: Option<String>,
    kind: Option<MetadataDependencyKind>,
    pub source: Option<String>,
}

impl PackageDependency {
    #[must_use]
    pub fn kind(&self) -> DependencyKind {
        match self.kind {
            None => DependencyKind::Normal,
            Some(MetadataDependencyKind::Build) => DependencyKind::Build,
            Some(MetadataDependencyKind::Dev) => DependencyKind::Dev,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MetadataDependencyKind {
    Build,
    Dev,
}
