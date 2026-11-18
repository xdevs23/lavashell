// SPDX-License-Identifier: GPL-3.0-only
use super::{InheritedTable, read_manifest};
use crate::error::Error;
use crate::findings::{Finding, Rule};
use crate::metadata::MANIFEST;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LISTING: &str = "nix/cargo-workspaces.toml";

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct CargoWorkspaces {
    pub(super) inherited_tables: Vec<InheritedTable>,
    directories: BTreeMap<String, String>,
}

impl CargoWorkspaces {
    pub(super) fn read(root: &Path) -> Result<Self, Error> {
        read_manifest(&root.join(LISTING))
    }

    pub(super) fn findings(&self, root: &Path, nested_directories: &[PathBuf]) -> Vec<Finding> {
        let found: BTreeSet<PathBuf> = std::iter::once(root.to_path_buf())
            .chain(nested_directories.iter().cloned())
            .collect();
        let listed: BTreeSet<PathBuf> = (self.directories.values())
            .map(|directory| root.join(directory))
            .collect();
        let unlisted = found.difference(&listed).map(|directory| Finding {
            path: directory.join(MANIFEST),
            line: None,
            rule: Rule::UnlistedWorkspace { listing: LISTING },
        });
        let without_workspace = (self.directories.values())
            .filter(|directory| !found.contains(&root.join(directory)))
            .map(|directory| Finding {
                path: root.join(LISTING),
                line: None,
                rule: Rule::ListedWithoutWorkspace {
                    directory: directory.clone(),
                },
            });
        unlisted.chain(without_workspace).collect()
    }
}
