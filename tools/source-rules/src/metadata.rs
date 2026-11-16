// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

    pub fn read() -> Result<Self, Error> {
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--format-version",
                "1",
            ])
            .stderr(Stdio::inherit())
            .output()
            .map_err(Error::StartMetadata)?;
        if !output.status.success() {
            return Err(Error::Metadata(output.status));
        }
        serde_json::from_slice(&output.stdout).map_err(Error::ReadMetadata)
    }
}
