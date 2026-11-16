// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/manifest.html
use crate::dependency_kind::DependencyKind;
use crate::fixed_flag::FixedFlag;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubManifest {
    pub package: StubPackage,
    pub lib: StubLibrary,
    pub features: DeclaredFeatures,
    #[serde(flatten)]
    pub dependencies: BTreeMap<DependencyKind, BTreeMap<String, DependencyRequest>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubPackage {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub autobins: FixedFlag<false>,
    pub autotests: FixedFlag<false>,
    pub autoexamples: FixedFlag<false>,
    pub autobenches: FixedFlag<false>,
    pub build: BuildScript,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubLibrary {
    pub path: String,
    #[serde(
        rename = "proc-macro",
        default,
        skip_serializing_if = "<&bool as std::ops::Not>::not"
    )]
    pub proc_macro: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclaredFeatures(pub BTreeSet<String>);

impl Serialize for DeclaredFeatures {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|feature| (feature, [(); 0])))
    }
}

impl<'de> Deserialize<'de> for DeclaredFeatures {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let declared = BTreeMap::<String, Vec<String>>::deserialize(deserializer)?;
        match declared.iter().find(|(_, enabled)| !enabled.is_empty()) {
            Some((feature, _)) => Err(de::Error::custom(format!(
                "the stub feature {feature} enables other features"
            ))),
            None => Ok(Self(declared.into_keys().collect())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BuildScript(pub Option<String>);

impl Serialize for BuildScript {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Some(path) => serializer.serialize_str(path),
            None => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for BuildScript {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(BuildScriptVisitor)
    }
}

struct BuildScriptVisitor;

impl Visitor<'_> for BuildScriptVisitor {
    type Value = BuildScript;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the path of a build script, or false")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<BuildScript, E> {
        if value {
            Err(E::invalid_value(de::Unexpected::Bool(value), &self))
        } else {
            Ok(BuildScript(None))
        }
    }

    fn visit_str<E: de::Error>(self, path: &str) -> Result<BuildScript, E> {
        Ok(BuildScript(Some(path.to_owned())))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRequest {
    pub package: String,
    #[serde(rename = "default-features")]
    pub default_features: FixedFlag<false>,
    pub features: Vec<String>,
    #[serde(flatten)]
    pub source: RequestSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestSource {
    Path { path: String },
    Version { version: ExactVersion },
}

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
