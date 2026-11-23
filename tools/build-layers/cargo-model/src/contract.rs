// SPDX-License-Identifier: GPL-3.0-only
use crate::projection::Layer;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionContract {
    pub profile: String,
    pub target: String,
    pub projection_directory: String,
    pub engine_root: String,
    pub native_directory: String,
    pub layers: Vec<Layer>,
    pub packaged_crates: BTreeMap<String, String>,
    pub source_files: SourceFiles,
    pub stub: StubLayout,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFiles {
    pub every_package: Vec<String>,
    pub by_member: BTreeMap<String, Vec<String>>,
}

impl SourceFiles {
    pub fn all(&self) -> impl Iterator<Item = &String> {
        self.every_package
            .iter()
            .chain(self.by_member.values().flatten())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StubLayout {
    pub version: String,
    pub library: String,
    pub build_script: String,
    pub main_source: String,
    pub library_source: String,
}
