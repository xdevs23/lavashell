// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::file_tree::walk;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const STUBBED_PACKAGE: &str = "stubbed-member";
const STUBBED_CRATE: &str = "stubbed_member";
const CONSUMER_PACKAGE: &str = "consumer";
const PROFILE: &str = "release";
const WORKSPACE: [(&str, &str); 6] = [
    (
        "Cargo.toml",
        "[workspace]\nmembers = [\"consumer\", \"stubbed-member\"]\nresolver = \"3\"\n",
    ),
    (
        "stubbed-member/Cargo.toml",
        "[package]\nname = \"stubbed-member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ),
    ("stubbed-member/src/lib.rs", ""),
    (
        "consumer/Cargo.toml",
        concat!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            "[dependencies]\nstubbed-member = { path = \"../stubbed-member\" }\n",
            "[build-dependencies]\nstubbed-member = { path = \"../stubbed-member\" }\n",
        ),
    ),
    ("consumer/build.rs", "fn main() {}\n"),
    ("consumer/src/lib.rs", ""),
];

#[derive(Serialize)]
struct LayerFixture<'fixture> {
    layer: &'fixture Path,
    stubbed: &'static str,
    consumer: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerScripts {
    build_layer: String,
    restore: String,
}

fn run(directory: &Path, command: &mut Command) -> Result<(), FixtureError> {
    let output = command.current_dir(directory).output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn bash(script: &str) -> Command {
    let mut command = Command::new("bash");
    let _command: &mut Command = command.args(["-euo", "pipefail", "-c", script]);
    command
}

fn write_workspace(workspace: &Path) -> Result<(), FixtureError> {
    for (relative, contents) in WORKSPACE {
        let path = workspace.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(workspace))?;
        fs::write(path, contents)?;
    }
    Ok(())
}

fn mentions(path: &Path, name: &str) -> bool {
    path.to_string_lossy().contains(name)
}

#[test]
fn a_restored_layer_holds_no_artifact_of_a_stubbed_member_built_for_the_host_or_the_target()
-> Result<(), FixtureError> {
    let root = tempfile::tempdir()?;
    let [workspace, layer, consumer] =
        ["workspace", "layer", "consumer"].map(|name| root.path().join(name));
    write_workspace(&workspace)?;
    fs::create_dir(&consumer)?;
    let scripts: LayerScripts = evaluate(
        "stubbed-member-removal.nix",
        &LayerFixture {
            layer: &layer,
            stubbed: STUBBED_PACKAGE,
            consumer: CONSUMER_PACKAGE,
        },
    )?;
    run(
        &workspace,
        Command::new("cargo").args(["generate-lockfile", "--offline"]),
    )?;
    run(
        &workspace,
        bash(&scripts.build_layer)
            .env("CARGO_TARGET_DIR", workspace.join("target"))
            .env("NIX_BUILD_TOP", root.path())
            .env("cargoBuildType", PROFILE)
            .env("out", &layer),
    )?;
    run(&consumer, &mut bash(&scripts.restore))?;
    let target = consumer.join("target");
    let restored: Vec<PathBuf> = walk(&target)?
        .into_iter()
        .map(|path| path.strip_prefix(&target).unwrap_or(&path).to_owned())
        .collect();
    let built_for_host = |path: &Path| path.starts_with(PROFILE);
    assert!(
        restored
            .iter()
            .any(|path| built_for_host(path) && mentions(path, CONSUMER_PACKAGE)),
        "the layer holds no build script of the consumer, so no stubbed member was built for the host: {restored:?}"
    );
    assert!(
        restored
            .iter()
            .any(|path| !built_for_host(path) && mentions(path, CONSUMER_PACKAGE)),
        "the layer holds no library of the consumer built for the target: {restored:?}"
    );
    let stubbed: Vec<&PathBuf> = restored
        .iter()
        .filter(|path| mentions(path, STUBBED_PACKAGE) || mentions(path, STUBBED_CRATE))
        .collect();
    assert!(stubbed.is_empty(), "{stubbed:?}");
    Ok(())
}
