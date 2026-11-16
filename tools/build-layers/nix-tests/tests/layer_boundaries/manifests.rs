// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/manifest.html
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use toml::Table;

#[derive(Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub workspace: Workspace,
    #[serde(flatten)]
    pub rest: Table,
}

#[derive(Serialize, Deserialize)]
pub struct Workspace {
    pub members: Vec<String>,
    pub package: WorkspacePackage,
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(flatten)]
    pub rest: Table,
}

#[derive(Serialize, Deserialize)]
pub struct WorkspacePackage {
    pub version: String,
    #[serde(flatten)]
    pub rest: Table,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed(DetailedDependency),
}

#[derive(Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(flatten)]
    pub rest: Table,
}

impl Dependency {
    pub fn version_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Version(version) => Some(version),
            Self::Detailed(detailed) => detailed.version.as_mut(),
        }
    }

    pub fn features_mut(&mut self) -> Option<&mut Vec<String>> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(detailed) => Some(&mut detailed.features),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MemberManifest {
    pub package: MemberPackage,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(flatten)]
    pub rest: Table,
}

#[derive(Serialize, Deserialize)]
pub struct MemberPackage {
    pub name: String,
    pub version: Inheritable,
    #[serde(flatten)]
    pub rest: Table,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Inheritable {
    Value(String),
    Workspace { workspace: bool },
}
