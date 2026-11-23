// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use clap::ValueEnum;
use lavashell_command_output::json::{CommandError, successful_output};
use lavashell_plan_check::stub_layer_reuse::CheckedBuild;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

const STUB_LAYER_PACKAGE: &str = "planning";
const PLANNED_PACKAGE_ID: &str =
    "registry+https://github.com/rust-lang/crates.io-index#planned@0.1.0";
const BUILD_SETTING: &str = "PLANNED_BUILD_SETTING";
const STUB_LAYER_SETTING: &str = "stub layer";
const BUILD_HOOK_LINE: &str = "Executing cargoBuildHook\n";
const WORKSPACE: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        "[workspace]\nmembers = [\"planning\", \"reusing\", \"recompiling\", \"widening\"]\nresolver = \"3\"\n",
    ),
    (
        ".cargo/config.toml",
        concat!(
            "[source.crates-io]\nreplace-with = \"fixture-vendor\"\n",
            "[source.fixture-vendor]\ndirectory = \"vendor\"\n",
        ),
    ),
    (
        "vendor/planned/Cargo.toml",
        concat!(
            "[package]\nname = \"planned\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[features]\nunplanned = []\n",
            "[dependencies]\nshared = \"0.1.0\"\n",
        ),
    ),
    (
        "vendor/planned/.cargo-checksum.json",
        "{\"files\":{},\"package\":null}",
    ),
    (
        "vendor/planned/build.rs",
        "fn main() {\n    println!(\"cargo::rerun-if-env-changed=PLANNED_BUILD_SETTING\");\n}\n",
    ),
    ("vendor/planned/src/lib.rs", ""),
    (
        "vendor/shared/Cargo.toml",
        concat!(
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[features]\nwidened = []\n",
        ),
    ),
    (
        "vendor/shared/.cargo-checksum.json",
        "{\"files\":{},\"package\":null}",
    ),
    ("vendor/shared/src/lib.rs", ""),
    (
        "planning/Cargo.toml",
        concat!(
            "[package]\nname = \"planning\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[dependencies]\nplanned = \"0.1.0\"\n",
        ),
    ),
    ("planning/src/lib.rs", ""),
    (
        "reusing/Cargo.toml",
        concat!(
            "[package]\nname = \"reusing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[dependencies]\nplanned = \"0.1.0\"\n",
        ),
    ),
    ("reusing/src/lib.rs", ""),
    (
        "recompiling/Cargo.toml",
        concat!(
            "[package]\nname = \"recompiling\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[dependencies]\nplanned = { version = \"0.1.0\", features = [\"unplanned\"] }\n",
        ),
    ),
    ("recompiling/src/lib.rs", ""),
    (
        "widening/Cargo.toml",
        concat!(
            "[package]\nname = \"widening\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[dependencies]\nplanned = \"0.1.0\"\n",
            "shared = { version = \"0.1.0\", features = [\"widened\"] }\n",
        ),
    ),
    ("widening/src/lib.rs", ""),
];

#[derive(Debug, thiserror::Error)]
enum FixtureError {
    #[error(transparent)]
    File(#[from] std::io::Error),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("the checked build {0:?} has no command line value")]
    HiddenCheckedBuild(CheckedBuild),
}

struct Workspace(TempDir);

impl Workspace {
    fn new() -> Result<Self, FixtureError> {
        let directory = tempfile::tempdir()?;
        for (relative, contents) in WORKSPACE {
            let path = directory.path().join(relative);
            fs::create_dir_all(path.parent().unwrap_or(directory.path()))?;
            fs::write(path, contents)?;
        }
        Ok(Self(directory))
    }

    fn record_build(
        &self,
        package: &str,
        build_setting: &str,
        record: &str,
    ) -> Result<PathBuf, FixtureError> {
        let output = successful_output(
            Command::new(env!("CARGO"))
                .args([
                    "build",
                    "--offline",
                    "--message-format=json-render-diagnostics",
                ])
                .args(["--package", package])
                .current_dir(self.0.path())
                .env("CARGO_TARGET_DIR", self.0.path().join("target"))
                .env(BUILD_SETTING, build_setting),
        )?;
        let path = self.0.path().join(record);
        fs::write(&path, [BUILD_HOOK_LINE.as_bytes(), &output.stdout].concat())?;
        Ok(path)
    }
}

struct StubLayer {
    workspace: Workspace,
    record: PathBuf,
}

impl StubLayer {
    fn build() -> Result<Self, FixtureError> {
        let workspace = Workspace::new()?;
        let record =
            workspace.record_build(STUB_LAYER_PACKAGE, STUB_LAYER_SETTING, "stub-layer.jsonl")?;
        Ok(Self { workspace, record })
    }

    fn check_build_above(
        &self,
        package: &str,
        build_setting: &str,
    ) -> Result<Command, FixtureError> {
        self.check_build_of_kind_above(CheckedBuild::Consumer, package, build_setting)
    }

    fn check_build_of_kind_above(
        &self,
        checked_build: CheckedBuild,
        package: &str,
        build_setting: &str,
    ) -> Result<Command, FixtureError> {
        let build_record = self
            .workspace
            .record_build(package, build_setting, "build.jsonl")?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_lavashell-plan-check"));
        let _command: &mut Command = command
            .arg("--build")
            .arg(build_record)
            .arg("--stub-layer")
            .arg(&self.record)
            .arg("--checked-build")
            .arg(
                checked_build
                    .to_possible_value()
                    .ok_or(FixtureError::HiddenCheckedBuild(checked_build))?
                    .get_name(),
            );
        Ok(command)
    }
}

fn assert_refused(check: &Output, report: &str) {
    let errors = String::from_utf8_lossy(&check.stderr);
    assert!(!check.status.success(), "{errors}");
    assert!(errors.contains(report), "{errors}");
}

#[test]
fn a_build_reusing_the_planned_third_party_units_passes() -> Result<(), FixtureError> {
    let _check: Output = successful_output(
        &mut StubLayer::build()?.check_build_above("reusing", STUB_LAYER_SETTING)?,
    )?;
    Ok(())
}

#[test]
fn a_build_compiling_an_unplanned_variant_of_a_planned_crate_fails() -> Result<(), FixtureError> {
    let stub_layer = StubLayer::build()?;
    let check = stub_layer
        .check_build_above("recompiling", STUB_LAYER_SETTING)?
        .output()?;
    assert_refused(
        &check,
        &format!(
            "This build compiled the target planned of {PLANNED_PACKAGE_ID} with the features \
             [unplanned], which the stub layer recorded in {} built only with other features.",
            stub_layer.record.display()
        ),
    );
    Ok(())
}

#[test]
fn a_build_compiling_a_planned_unit_again_fails() -> Result<(), FixtureError> {
    let stub_layer = StubLayer::build()?;
    let check = stub_layer
        .check_build_above("reusing", "changed")?
        .output()?;
    assert_refused(
        &check,
        &format!(
            "This build compiled the target planned of {PLANNED_PACKAGE_ID} with the features [] \
             again, although the stub layer recorded in {} built that same unit under the same hash.",
            stub_layer.record.display()
        ),
    );
    Ok(())
}

#[test]
fn a_build_resolving_the_dependencies_of_a_planned_unit_differently_fails()
-> Result<(), FixtureError> {
    let stub_layer = StubLayer::build()?;
    let check = stub_layer
        .check_build_above("widening", STUB_LAYER_SETTING)?
        .output()?;
    assert_refused(
        &check,
        &format!(
            "This build compiled the target planned of {PLANNED_PACKAGE_ID} with the features [] \
             under another hash than the stub layer recorded in {}, because its dependencies \
             resolved differently.",
            stub_layer.record.display()
        ),
    );
    Ok(())
}

#[test]
fn a_stub_layer_compiling_a_variant_the_stub_layer_below_never_built_passes()
-> Result<(), FixtureError> {
    let _check: Output = successful_output(&mut StubLayer::build()?.check_build_of_kind_above(
        CheckedBuild::StubLayer,
        "recompiling",
        STUB_LAYER_SETTING,
    )?)?;
    Ok(())
}

#[test]
fn a_stub_layer_resolving_the_dependencies_of_a_unit_below_differently_fails()
-> Result<(), FixtureError> {
    let stub_layer = StubLayer::build()?;
    let check = stub_layer
        .check_build_of_kind_above(CheckedBuild::StubLayer, "widening", STUB_LAYER_SETTING)?
        .output()?;
    assert_refused(
        &check,
        &format!(
            "This build compiled the target planned of {PLANNED_PACKAGE_ID} with the features [] \
             under another hash than the stub layer recorded in {}, because its dependencies \
             resolved differently.",
            stub_layer.record.display()
        ),
    );
    Ok(())
}
