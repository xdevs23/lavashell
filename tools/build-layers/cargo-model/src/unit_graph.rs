// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph
use crate::package_id::PackageId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub opt_level: String,
    pub lto: String,
    pub codegen_backend: Option<String>,
    pub codegen_units: Option<u32>,
    pub debuginfo: Option<DebugInfo>,
    #[serde(flatten)]
    pub runtime_checks: RuntimeChecks,
    #[serde(flatten)]
    pub compilation: CompilationSettings,
    pub panic: String,
    pub split_debuginfo: Option<String>,
    pub strip: Strip,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeChecks {
    pub debug_assertions: bool,
    pub overflow_checks: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CompilationSettings {
    pub rpath: bool,
    pub incremental: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DebugInfo {
    Level(u8),
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strip {
    Deferred(StripLevel),
    Resolved(StripLevel),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StripLevel {
    None,
    Named(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Bin,
    Lib,
    Rlib,
    Dylib,
    Cdylib,
    Staticlib,
    ProcMacro,
    CustomBuild,
    Test,
    Bench,
    Example,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrateType {
    Bin,
    Lib,
    Rlib,
    Dylib,
    Cdylib,
    Staticlib,
    ProcMacro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UnitMode {
    #[serde(rename = "build")]
    Build,
    #[serde(rename = "check")]
    Check,
    #[serde(rename = "test")]
    Test,
    #[serde(rename = "doc")]
    Doc,
    #[serde(rename = "doctest")]
    DocTest,
    #[serde(rename = "docscrape")]
    DocScrape,
    #[serde(rename = "run-custom-build")]
    RunCustomBuild,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TargetIdentity {
    pub name: String,
    pub kind: Vec<TargetKind>,
    pub crate_types: Vec<CrateType>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitIdentity {
    pub package: PackageId,
    pub target: TargetIdentity,
    pub profile: Profile,
    pub platform: Option<String>,
    pub mode: UnitMode,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitRecord {
    pub unit: UnitIdentity,
    pub dependencies: BTreeSet<UnitIdentity>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Edge {
    pub index: usize,
    pub extern_crate_name: String,
    #[serde(default)]
    pub noprelude: bool,
    #[serde(default)]
    pub nounused: bool,
    #[serde(default)]
    pub public: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Unit {
    pub pkg_id: PackageId,
    pub target: TargetIdentity,
    pub profile: Profile,
    pub platform: Option<String>,
    pub mode: UnitMode,
    pub features: Vec<String>,
    pub dependencies: Vec<Edge>,
}

impl Unit {
    #[must_use]
    pub fn identity(&self) -> UnitIdentity {
        UnitIdentity {
            package: self.pkg_id.clone(),
            target: self.target.clone(),
            profile: self.profile.clone(),
            platform: self.platform.clone(),
            mode: self.mode,
            features: self.features.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnitGraph {
    pub units: Vec<Unit>,
    pub roots: Vec<usize>,
}

impl UnitGraph {
    pub fn unit(&self, index: usize) -> Result<&Unit, UnitIndexError> {
        self.units.get(index).ok_or(UnitIndexError { index })
    }

    pub fn dependencies<'graph>(
        &'graph self,
        unit: &'graph Unit,
    ) -> impl Iterator<Item = Result<&'graph Unit, UnitIndexError>> {
        unit.dependencies.iter().map(|edge| self.unit(edge.index))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the unit graph has no unit at index {index}")]
pub struct UnitIndexError {
    pub index: usize,
}

#[cfg(test)]
mod tests;
