// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The packages of the workspace in the current directory, as `cargo metadata` reports them.
#[derive(Deserialize)]
pub struct Workspace {
    pub workspace_root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Deserialize)]
pub struct Package {
    pub targets: Vec<Target>,
}

/// A crate that Cargo builds from the package: a library, a program, a test, an example, a benchmark or a build
/// script.
#[derive(Deserialize)]
pub struct Target {
    pub src_path: PathBuf,
}

impl Workspace {
    /// Reads the workspace with `cargo metadata --no-deps`. Without dependency resolution, Cargo needs neither the
    /// network nor the package registry.
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
