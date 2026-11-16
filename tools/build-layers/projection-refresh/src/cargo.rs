// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::read_toml;
use lavashell_cargo_model::lock::CargoLock;
use lavashell_cargo_model::metadata::CargoMetadata;
use lavashell_cargo_model::unit_graph::UnitGraph;
use lavashell_command_output::json::read_json_output;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Resolve,
    Frozen,
}

impl LockMode {
    fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Resolve => &[],
            Self::Frozen => &["--frozen"],
        }
    }
}

const PROGRAM: &str = "cargo";

pub struct Cargo {
    pub target: String,
    pub profile: String,
}

pub struct ResolvedGraph {
    pub graph: UnitGraph,
    pub lock: CargoLock,
}

impl Cargo {
    pub fn unit_graph(
        &self,
        directory: &Path,
        packages: &[String],
        mode: LockMode,
    ) -> Result<ResolvedGraph, Error> {
        let graph = read_json_output(
            Command::new(PROGRAM)
                .current_dir(directory)
                .env("RUSTC_BOOTSTRAP", "1")
                .env("CARGO_TARGET_DIR", directory.join("target"))
                .args([
                    "build",
                    "--unit-graph",
                    "-Z",
                    "unstable-options",
                    "--offline",
                ])
                .args(["--profile", &self.profile, "--target", &self.target])
                .args(mode.arguments())
                .args(packages.iter().flat_map(|package| ["--package", package])),
        )
        .map_err(Error::Cargo)?;
        let lock = read_toml(&directory.join("Cargo.lock"))?;
        Ok(ResolvedGraph { graph, lock })
    }

    pub fn metadata(&self, directory: &Path) -> Result<CargoMetadata, Error> {
        read_json_output(
            Command::new(PROGRAM)
                .current_dir(directory)
                .args(["metadata", "--offline", "--frozen", "--format-version", "1"])
                .args(["--filter-platform", &self.target]),
        )
        .map_err(Error::Cargo)
    }
}
