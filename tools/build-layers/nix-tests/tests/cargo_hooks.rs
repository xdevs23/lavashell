// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_command_output::nix::NixEvaluator;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::repository_root;
use serde::Deserialize;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const HARNESS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/cargo-hook-harness.sh"
);
const HOOKS: &str = "pkgs/build-support/rust/hooks";
const TARGET: &str = "x86_64-unknown-linux-gnu";
const ARTIFACT_LOG: &str = "cargo-artifacts.jsonl";

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    BuildFinished {
        success: bool,
    },
    #[serde(other)]
    Other,
}

enum CargoOutcome {
    Succeeds,
    FailsWith(u8),
}

enum ArtifactLog {
    Writable,
    Full,
}

struct HookDirectory(TempDir);

impl HookDirectory {
    fn new() -> Result<Self, FixtureError> {
        let directory = tempfile::tempdir()?;
        let hooks = NixEvaluator::from_environment()?.nixpkgs().join(HOOKS);
        for (source, destination) in [
            ("cargo-build-hook.sh", "build-hook.sh"),
            ("cargo-install-hook.sh", "install-hook.sh"),
        ] {
            let contents = fs::read_to_string(hooks.join(source))?
                .replace("@rustcTargetSpec@", TARGET)
                .replace("@targetSubdirectory@", TARGET)
                .replace("@setEnv@", "");
            fs::write(directory.path().join(destination), contents)?;
        }
        let _bytes: u64 = fs::copy(HARNESS, directory.path().join("harness.sh"))?;
        Ok(Self(directory))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn run(&self, cargo: &CargoOutcome, log: &ArtifactLog) -> Result<Output, FixtureError> {
        if let ArtifactLog::Full = log {
            symlink("/dev/full", self.path().join(ARTIFACT_LOG))?;
        }
        let status = match cargo {
            CargoOutcome::Succeeds => 0,
            CargoOutcome::FailsWith(status) => *status,
        };
        Ok(Command::new("bash")
            .args(["-eu", "-c", "source harness.sh"])
            .current_dir(self.path())
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").ok_or(FixtureError::PathUnset)?,
            )
            .env(
                "PHASE",
                repository_root()?.join("nix/cargo-build-record.sh"),
            )
            .env("CARGO_FIXTURE_STATUS", status.to_string())
            .output()?)
    }
}

#[test]
fn exports_install_bins_and_complete_log_survive() -> Result<(), FixtureError> {
    let directory = HookDirectory::new()?;
    let output = directory.run(&CargoOutcome::Succeeds, &ArtifactLog::Writable)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("CHECK_PKG_CONFIG=retained-pkg-config"));
    assert!(!stdout.contains("CHECK_BINS=missing"));
    assert!(directory.path().join("out/bin/fixture").is_file());
    let log = fs::read_to_string(directory.path().join(ARTIFACT_LOG))?;
    let messages: Vec<CargoMessage> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or(CargoMessage::Other))
        .filter(|message| *message != CargoMessage::Other)
        .collect();
    assert_eq!(messages, [CargoMessage::BuildFinished { success: true }]);
    assert!(log.contains("Finished cargoBuildHook"));
    Ok(())
}

#[test]
fn cargo_failure_is_propagated_before_install() -> Result<(), FixtureError> {
    let output = HookDirectory::new()?.run(&CargoOutcome::FailsWith(42), &ArtifactLog::Writable)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout.contains("INSTALL_FINISHED"));
    assert!(!stdout.contains("cargoInstallPostBuildHook"));
    Ok(())
}

#[test]
fn tee_write_failure_is_propagated_before_install() -> Result<(), FixtureError> {
    let output = HookDirectory::new()?.run(&CargoOutcome::Succeeds, &ArtifactLog::Full)?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No space left on device"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("INSTALL_FINISHED"));
    Ok(())
}
