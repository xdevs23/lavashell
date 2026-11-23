// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use toml::{Table, toml};

#[derive(Serialize)]
struct TargetArguments<'arguments> {
    directory: &'arguments Path,
    manifest: Table,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct StubTarget {
    path: String,
    main: bool,
}

fn target(path: &str, main: bool) -> StubTarget {
    StubTarget {
        path: path.to_owned(),
        main,
    }
}

#[test]
fn named_targets_without_a_path_use_the_default_directories_of_cargo() -> Result<(), FixtureError> {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("src"))?;
    fs::write(directory.path().join("src/lib.rs"), "")?;
    let manifest = toml! {
        [package]
        name = "crate-with-targets"

        [[bin]]
        name = "tool"

        [[test]]
        name = "termination"
        harness = false

        [[bench]]
        name = "throughput"

        [[example]]
        name = "demo"
        path = "demos/demo.rs"
    };
    let mut targets: Vec<StubTarget> = evaluate(
        "cargo-targets.nix",
        &TargetArguments {
            directory: directory.path(),
            manifest,
        },
    )?;
    targets.sort_by(|left, right| left.path.cmp(&right.path));
    assert_eq!(
        targets,
        [
            target("benches/throughput.rs", true),
            target("demos/demo.rs", true),
            target("src/bin/tool.rs", true),
            target("src/lib.rs", false),
            target("tests/termination.rs", true),
        ]
    );
    Ok(())
}

#[test]
fn an_example_built_as_a_library_gets_no_main_function() -> Result<(), FixtureError> {
    let directory = tempfile::tempdir()?;
    let manifest = toml! {
        [package]
        name = "crate-with-a-library-example"

        [[example]]
        name = "plugin"
        path = "plugin/lib.rs"
        crate-type = ["cdylib"]

        [[example]]
        name = "tool"
        path = "tool/main.rs"
        crate-type = ["bin"]
    };
    let mut targets: Vec<StubTarget> = evaluate(
        "cargo-targets.nix",
        &TargetArguments {
            directory: directory.path(),
            manifest,
        },
    )?;
    targets.sort_by(|left, right| left.path.cmp(&right.path));
    assert_eq!(
        targets,
        [target("plugin/lib.rs", false), target("tool/main.rs", true)]
    );
    Ok(())
}
