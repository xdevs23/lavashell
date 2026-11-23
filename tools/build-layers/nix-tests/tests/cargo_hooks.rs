// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_command_output::json::successful_output;
use lavashell_command_output::nix::NixEvaluator;
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const HARNESS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/cargo-hook-harness.sh"
);
const FAKE_CARGO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/cargo.sh");
const HOOKS: &str = "pkgs/build-support/rust/hooks";
const TARGET: &str = "x86_64-unknown-linux-gnu";
const EXECUTABLE: u32 = 0o755;
const BUILD: &[&str] = &["buildPhase"];
const BUILD_TEST_AND_LINT: &[&str] = &["buildPhase", "checkPhase", "clippyPhase"];

#[derive(Serialize)]
struct TreeArguments {
    root: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Phases {
    build_phase: String,
    check_phase: String,
    clippy_phase: String,
    cargo_build_flags: Vec<String>,
    cargo_test_flags: Vec<String>,
    cargo_clippy_flags: Vec<String>,
}

impl Phases {
    fn environment(&self) -> [(&'static str, String); 6] {
        [
            ("buildPhase", self.build_phase.clone()),
            ("checkPhase", self.check_phase.clone()),
            ("clippyPhase", self.clippy_phase.clone()),
            ("cargoBuildFlags", self.cargo_build_flags.join(" ")),
            ("cargoTestFlags", self.cargo_test_flags.join(" ")),
            ("cargoClippyFlags", self.cargo_clippy_flags.join(" ")),
        ]
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordedPhases {
    file_name: String,
    package: Phases,
    stub_layer: Phases,
}

fn recorded_phases() -> Result<RecordedPhases, FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("repository");
    copy_repository(&root)?;
    evaluate("recorded-phases.nix", &TreeArguments { root })
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    CompilerArtifact {
        target: CompiledTarget,
    },
    BuildFinished {
        success: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct CompiledTarget {
    name: String,
}

enum CargoOutcome {
    Succeeds,
    FailsWith(u8),
}

enum ArtifactLog {
    Writable,
    Full,
}

struct HookDirectory {
    directory: TempDir,
    phases: RecordedPhases,
}

impl HookDirectory {
    fn new() -> Result<Self, FixtureError> {
        let phases = recorded_phases()?;
        let directory = tempfile::tempdir()?;
        let hooks = NixEvaluator::from_environment()?.nixpkgs().join(HOOKS);
        for (source, destination) in [
            ("cargo-build-hook.sh", "build-hook.sh"),
            ("cargo-install-hook.sh", "install-hook.sh"),
            ("cargo-check-hook.sh", "check-hook.sh"),
        ] {
            let contents = fs::read_to_string(hooks.join(source))?
                .replace("@rustcTargetSpec@", TARGET)
                .replace("@targetSubdirectory@", TARGET)
                .replace("@setEnv@", "");
            fs::write(directory.path().join(destination), contents)?;
        }
        let _bytes: u64 = fs::copy(HARNESS, directory.path().join("harness.sh"))?;
        let cargo = directory.path().join("bin/cargo");
        fs::create_dir(directory.path().join("bin"))?;
        let _bytes: u64 = fs::copy(FAKE_CARGO, &cargo)?;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(EXECUTABLE))?;
        Ok(Self { directory, phases })
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn record(&self) -> PathBuf {
        self.path().join(&self.phases.file_name)
    }

    fn command(
        &self,
        cargo: &CargoOutcome,
        log: &ArtifactLog,
        phases: &Phases,
        run: &[&str],
    ) -> Result<Command, FixtureError> {
        if let ArtifactLog::Full = log {
            symlink("/dev/full", self.record())?;
        }
        let status = match cargo {
            CargoOutcome::Succeeds => 0,
            CargoOutcome::FailsWith(status) => *status,
        };
        let mut command = Command::new("bash");
        let _command: &mut Command = command
            .args(["-eu", "-c", "source harness.sh"])
            .current_dir(self.path())
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").ok_or(FixtureError::PathUnset)?,
            )
            .envs(phases.environment())
            .env("PHASES", run.join(" "))
            .env("CARGO_FIXTURE_STATUS", status.to_string());
        Ok(command)
    }
}

fn cargo_messages(log: &str) -> Vec<CargoMessage> {
    log.lines()
        .map(|line| serde_json::from_str(line).unwrap_or(CargoMessage::Other))
        .filter(|message| *message != CargoMessage::Other)
        .collect()
}

#[test]
fn exports_install_bins_and_complete_log_survive() -> Result<(), FixtureError> {
    let directory = HookDirectory::new()?;
    let output = successful_output(&mut directory.command(
        &CargoOutcome::Succeeds,
        &ArtifactLog::Writable,
        &directory.phases.package,
        BUILD,
    )?)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CHECK_PKG_CONFIG=retained-pkg-config"));
    assert!(!stdout.contains("CHECK_BINS=missing"));
    assert!(directory.path().join("out/bin/fixture").is_file());
    let log = fs::read_to_string(directory.record())?;
    assert_eq!(
        cargo_messages(&log),
        [
            CargoMessage::CompilerArtifact {
                target: CompiledTarget {
                    name: "build".to_owned()
                }
            },
            CargoMessage::BuildFinished { success: true }
        ]
    );
    assert!(log.contains("Finished cargoBuildHook"));
    Ok(())
}

#[test]
fn each_recorded_cargo_run_appends_the_messages_it_prints_to_the_log() -> Result<(), FixtureError> {
    let directory = HookDirectory::new()?;
    let _output = successful_output(&mut directory.command(
        &CargoOutcome::Succeeds,
        &ArtifactLog::Writable,
        &directory.phases.stub_layer,
        BUILD_TEST_AND_LINT,
    )?)?;
    let compiled: Vec<String> = cargo_messages(&fs::read_to_string(directory.record())?)
        .into_iter()
        .filter_map(|message| match message {
            CargoMessage::CompilerArtifact { target } => Some(target.name),
            _ => None,
        })
        .collect();
    assert_eq!(compiled, ["build", "check", "test", "clippy"]);
    Ok(())
}

#[test]
fn cargo_failure_is_propagated_before_install() -> Result<(), FixtureError> {
    let directory = HookDirectory::new()?;
    let output = directory
        .command(
            &CargoOutcome::FailsWith(42),
            &ArtifactLog::Writable,
            &directory.phases.package,
            BUILD,
        )?
        .output()?;
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
    let directory = HookDirectory::new()?;
    let output = directory
        .command(
            &CargoOutcome::Succeeds,
            &ArtifactLog::Full,
            &directory.phases.package,
            BUILD,
        )?
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No space left on device"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("INSTALL_FINISHED"));
    Ok(())
}
