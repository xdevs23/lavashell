// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_command_output::json::successful_output;
use lavashell_command_output::nix::NixEvaluator;
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::toml_files::read_toml;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const HARNESS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/cargo-setup-harness.sh"
);
const HOOK: &str = "pkgs/build-support/rust/hooks/cargo-setup-hook.sh";
const CARGO_CONFIGURATION: &str = ".cargo/config.toml";

#[derive(Serialize)]
struct NoArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VendoredSource {
    vendor_directory: String,
    source_root: String,
    cargo_vendor_dir: String,
    unpacked_directories: Vec<String>,
}

#[derive(Deserialize)]
struct VendorConfiguration {
    directory: PathBuf,
}

#[test]
fn cargo_reads_the_vendored_crates_where_the_unpack_phase_left_them() -> Result<(), FixtureError> {
    let vendored: VendoredSource = evaluate("vendored-source.nix", &NoArguments {})?;
    let hooks = tempfile::tempdir()?;
    let hook = hooks.path().join("cargo-setup-hook.sh");
    fs::write(
        &hook,
        fs::read_to_string(NixEvaluator::from_environment()?.nixpkgs().join(HOOK))?
            .replace("@defaultConfig@", "/dev/null")
            .replace("@cargoConfig@", ""),
    )?;
    let build = tempfile::tempdir()?;
    for directory in &vendored.unpacked_directories {
        fs::create_dir(build.path().join(directory))?;
    }
    let vendor = build.path().join(&vendored.vendor_directory);
    fs::create_dir(vendor.join(".cargo"))?;
    fs::write(
        vendor.join(CARGO_CONFIGURATION),
        "directory = \"@vendor@\"\n",
    )?;
    let _output = successful_output(
        Command::new("bash")
            .args(["-eu", HARNESS])
            .current_dir(build.path())
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").ok_or(FixtureError::PathUnset)?,
            )
            .env("TMPDIR", hooks.path())
            .env("CARGO_SETUP_HOOK", &hook)
            .env("sourceRoot", &vendored.source_root)
            .env("cargoVendorDir", &vendored.cargo_vendor_dir),
    )?;
    let configuration: VendorConfiguration = read_toml(&build.path().join(CARGO_CONFIGURATION))?;
    assert_eq!(configuration.directory, vendor.canonicalize()?);
    Ok(())
}
