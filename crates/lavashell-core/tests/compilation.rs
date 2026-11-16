// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Compiles,
    Fails(BTreeSet<String>),
}

impl Outcome {
    fn fixture_directory(&self) -> &'static str {
        match self {
            Self::Compiles => "compile_pass",
            Self::Fails(_) => "compile_fail",
        }
    }
}

fn fails_with(code: &str) -> Outcome {
    Outcome::Fails(BTreeSet::from([code.to_owned()]))
}

fn expected_outcomes() -> BTreeMap<&'static str, Outcome> {
    BTreeMap::from([
        ("const_local_key", fails_with("E0716")),
        ("dp_plus_number", fails_with("E0308")),
        ("dp_plus_px", fails_with("E0308")),
        ("dp_times_dp", fails_with("E0277")),
        ("negative_constant", fails_with("E0080")),
        ("number_into_dp", fails_with("E0277")),
        ("number_into_length", fails_with("E0277")),
        ("sp_into_dp", fails_with("E0277")),
        ("sp_plus_dp", fails_with("E0308")),
        ("unitless_font_size", fails_with("E0308")),
        ("static_local_key", Outcome::Compiles),
        ("valid_constant", Outcome::Compiles),
    ])
}

#[derive(Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    CompilerMessage {
        target: Target,
        message: Diagnostic,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Target {
    name: String,
}

#[derive(Deserialize)]
struct Diagnostic {
    level: Level,
    code: Option<ErrorCode>,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Level {
    Error,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ErrorCode {
    code: String,
}

#[derive(Debug, thiserror::Error)]
enum FixtureError {
    #[error("the fixture project cannot be written or checked")]
    Project(#[from] std::io::Error),
    #[error("the output of cargo check cannot be read")]
    CargoOutput(#[from] serde_json::Error),
    #[error("the manifest of the fixture project cannot be written")]
    Manifest(#[from] toml::ser::Error),
}

#[derive(Serialize)]
struct FixtureManifest<'a> {
    package: FixturePackage,
    workspace: EmptyWorkspace,
    dependencies: BTreeMap<&'static str, PathDependency>,
    bin: Vec<FixtureBinary<'a>>,
}

#[derive(Serialize)]
struct FixturePackage {
    name: &'static str,
    edition: &'static str,
    publish: bool,
}

#[derive(Serialize)]
struct EmptyWorkspace {}

#[derive(Serialize)]
struct PathDependency {
    path: PathBuf,
}

#[derive(Serialize)]
struct FixtureBinary<'a> {
    name: &'a str,
    path: PathBuf,
}

fn fixture_manifest(fixtures: &BTreeMap<&str, Outcome>) -> Result<String, FixtureError> {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = FixtureManifest {
        package: FixturePackage {
            name: "lavashell-core-fixtures",
            edition: "2024",
            publish: false,
        },
        workspace: EmptyWorkspace {},
        dependencies: BTreeMap::from([(
            "lavashell-core",
            PathDependency {
                path: core.to_path_buf(),
            },
        )]),
        bin: fixtures
            .iter()
            .map(|(name, outcome)| FixtureBinary {
                name,
                path: core
                    .join("tests")
                    .join(outcome.fixture_directory())
                    .join(format!("{name}.rs")),
            })
            .collect(),
    };
    Ok(toml::to_string(&manifest)?)
}

#[test]
fn each_fixture_compiles_or_fails_with_its_error_code() -> Result<(), FixtureError> {
    let expected = expected_outcomes();
    let project = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fixtures");
    std::fs::create_dir_all(&project)?;
    std::fs::write(project.join("Cargo.toml"), fixture_manifest(&expected)?)?;
    let _copied_byte_count_is_not_needed = std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        project.join("Cargo.lock"),
    )?;
    let output = Command::new(env!("CARGO"))
        .args(["check", "--bins", "--message-format=json", "--target-dir"])
        .arg(project.join("target"))
        .current_dir(&project)
        .output()?;
    let mut error_codes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for message in serde_json::Deserializer::from_slice(&output.stdout).into_iter() {
        if let CargoMessage::CompilerMessage { target, message } = message?
            && message.level == Level::Error
        {
            error_codes
                .entry(target.name)
                .or_default()
                .extend(message.code.map(|code| code.code));
        }
    }
    let observed: BTreeMap<_, _> = expected
        .keys()
        .map(|&name| match error_codes.remove(name) {
            Some(codes) => (name, Outcome::Fails(codes)),
            None => (name, Outcome::Compiles),
        })
        .collect();
    assert_eq!(
        observed,
        expected,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
