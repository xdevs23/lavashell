// SPDX-License-Identifier: GPL-3.0-only
use super::{CrateRule, InheritedTable, UnknownInheritedTable, read_toml};
use crate::error::Error;
use crate::findings::Finding;
use crate::metadata::MANIFEST;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LISTING: &str = "nix/cargo-workspaces.toml";

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Listing {
    inherited_tables: Vec<String>,
    directories: BTreeMap<String, String>,
}

pub(super) struct CargoWorkspaces {
    pub(super) inherited_tables: Vec<InheritedTable>,
    unknown_tables: Vec<String>,
    directories: BTreeMap<String, String>,
}

impl CargoWorkspaces {
    pub(super) fn read(root: &Path) -> Result<Self, Error> {
        let listing: Listing = read_toml(&root.join(LISTING), |path, source| {
            Error::ReadWorkspaceList { path, source }
        })?;
        let mut inherited_tables = Vec::new();
        let mut unknown_tables = Vec::new();
        for name in listing.inherited_tables {
            match name.parse() {
                Ok(table) => inherited_tables.push(table),
                Err(UnknownInheritedTable) => unknown_tables.push(name),
            }
        }
        Ok(Self {
            inherited_tables,
            unknown_tables,
            directories: listing.directories,
        })
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
            rule: CrateRule::UnlistedWorkspace { listing: LISTING }.into(),
        });
        let without_workspace = (self.directories.values())
            .filter(|directory| !found.contains(&root.join(directory)))
            .map(|directory| Finding {
                path: root.join(LISTING),
                line: None,
                rule: CrateRule::ListedWithoutWorkspace {
                    directory: directory.clone(),
                }
                .into(),
            });
        let unknown = self.unknown_tables.iter().map(|name| Finding {
            path: root.join(LISTING),
            line: None,
            rule: CrateRule::UnknownInheritedTable { name: name.clone() }.into(),
        });
        unlisted.chain(without_workspace).chain(unknown).collect()
    }
}
