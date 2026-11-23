// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/manifest.html
use crate::dependency_kind::DependencyKind;
use crate::fixed_flag::FixedFlag;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureActivation {
    pub version: ExactVersion,
    #[serde(rename = "default-features")]
    pub default_features: FixedFlag<false>,
    pub features: Vec<String>,
}

pub type FeatureActivations = BTreeMap<DependencyKind, BTreeMap<String, FeatureActivation>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExactVersion(pub String);

#[derive(Debug, thiserror::Error)]
#[error("the version requirement {requirement} does not ask for one exact version")]
pub struct InexactRequirement {
    pub requirement: String,
}

impl TryFrom<String> for ExactVersion {
    type Error = InexactRequirement;

    fn try_from(requirement: String) -> Result<Self, Self::Error> {
        match requirement.strip_prefix('=') {
            Some(version) => Ok(Self(version.to_owned())),
            None => Err(InexactRequirement { requirement }),
        }
    }
}

impl From<ExactVersion> for String {
    fn from(version: ExactVersion) -> Self {
        format!("={}", version.0)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PackageManifest {
    pub package: PackageSection,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PackageSection {
    pub name: String,
}

#[cfg(test)]
mod tests;
