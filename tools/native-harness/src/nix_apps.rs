// SPDX-License-Identifier: GPL-3.0-only
use crate::app_declarations::{WaylandApp, check_declared_spellings, read_wayland_app};
use crate::error::HarnessError;
use crate::program_path::find_on_path;
use lavashell_process_scope::run_checked_for_stdout;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

const REPOSITORY_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const NIX: &str = "nix";
const EXPERIMENTAL_FEATURES: [&str; 2] = ["--extra-experimental-features", "nix-command flakes"];

pub struct Flake {
    reference: String,
    nix: PathBuf,
}

impl Flake {
    pub fn of_repository() -> Result<Self, HarnessError> {
        let root = Path::new(REPOSITORY_ROOT)
            .canonicalize()
            .map_err(HarnessError::FindRepository)?;
        let location =
            Url::from_file_path(&root).map_err(|()| HarnessError::RepositoryUrl(root))?;
        Self::named(format!("git+{location}"))
    }

    pub fn named(reference: String) -> Result<Self, HarnessError> {
        let nix = find_on_path(NIX).ok_or(HarnessError::MissingTool(NIX))?;
        Ok(Self { reference, nix })
    }

    pub fn wayland_apps(&self, only: &[String]) -> Result<Vec<WaylandApp>, HarnessError> {
        check_declared_spellings()?;
        let system = self.current_system()?;
        let stdout = run_checked_for_stdout(self.nix().args([
            "eval",
            "--offline",
            "--json",
            &format!("{}#apps.{system}", self.reference),
        ]))?;
        let declarations: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(&stdout).map_err(HarnessError::UnreadableApps)?;
        let apps = (declarations.into_iter())
            .filter_map(|(name, declaration)| read_wayland_app(name, declaration).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let mut unknown: Vec<String> = (only.iter())
            .filter(|name| !apps.iter().any(|app| &app.name == *name))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            unknown.sort();
            unknown.dedup();
            return Err(HarnessError::UnknownApps(unknown));
        }
        Ok(apps
            .into_iter()
            .filter(|app| only.is_empty() || only.contains(&app.name))
            .collect())
    }

    pub fn run_command(&self, app: &WaylandApp) -> Command {
        let mut command = self.nix();
        let _chained_command = command
            .args([
                "run",
                "--offline",
                &format!("{}#{}", self.reference, app.name),
                "--",
            ])
            .args(&app.launch.arguments);
        command
    }

    fn nix(&self) -> Command {
        let mut command = Command::new(&self.nix);
        let _chained_command = command.args(EXPERIMENTAL_FEATURES);
        command
    }

    fn current_system(&self) -> Result<String, HarnessError> {
        let stdout = run_checked_for_stdout(self.nix().args([
            "eval",
            "--offline",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ]))?;
        String::from_utf8(stdout).map_err(HarnessError::UnreadableSystem)
    }
}

impl fmt::Display for Flake {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reference)
    }
}
