// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::repository_files::{FileKind, RepositoryFile};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const MANIFEST: &str = "Cargo.toml";

#[derive(Deserialize)]
struct AnyManifest {
    workspace: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
pub struct Workspace {
    pub workspace_root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Deserialize)]
pub struct Package {
    pub targets: Vec<Target>,
}

#[derive(Deserialize)]
pub struct Target {
    pub src_path: PathBuf,
}

impl Workspace {
    pub fn crate_roots(&self) -> Vec<PathBuf> {
        self.packages
            .iter()
            .flat_map(|package| &package.targets)
            .map(|target| target.src_path.clone())
            .collect()
    }

    pub fn read(directory: &Path) -> Result<Self, Error> {
        let output = Command::new("cargo")
            .current_dir(directory)
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--locked",
                "--format-version",
                "1",
            ])
            .stderr(Stdio::inherit())
            .output()
            .map_err(|source| Error::StartMetadata {
                directory: directory.to_owned(),
                source,
            })?;
        if !output.status.success() {
            return Err(Error::Metadata {
                directory: directory.to_owned(),
                status: output.status,
            });
        }
        serde_json::from_slice(&output.stdout).map_err(Error::ReadMetadata)
    }
}

pub fn nested_workspace_directories(
    files: &[RepositoryFile],
    root: &Path,
) -> Result<Vec<PathBuf>, Error> {
    let root_manifest = root.join(MANIFEST);
    files
        .iter()
        .filter_map(|file| match &file.kind {
            Some(FileKind::Source { text, .. })
                if file.path.ends_with(MANIFEST) && file.path != root_manifest =>
            {
                Some((&file.path, text))
            }
            _ => None,
        })
        .filter_map(|(path, text)| match toml::from_str::<AnyManifest>(text) {
            Ok(AnyManifest { workspace: None }) => None,
            Ok(AnyManifest { workspace: Some(_) }) => Some(
                path.parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| Error::NoParentDirectory { path: path.clone() }),
            ),
            Err(source) => Some(Err(Error::ReadManifest {
                path: path.clone(),
                source,
            })),
        })
        .collect()
}

pub fn crate_roots_of_every_workspace(
    root: &Workspace,
    nested_directories: &[PathBuf],
) -> Result<Vec<PathBuf>, Error> {
    let nested = (nested_directories.iter())
        .map(|directory| Workspace::read(directory))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(std::iter::once(root)
        .chain(&nested)
        .flat_map(Workspace::crate_roots)
        .collect())
}
