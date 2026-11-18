// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CARGO_WORKSPACES, Failure, ROOT_WORKSPACE_LISTING, crate_report_of_every_workspace, findings_of,
};
use crate::crate_manifests::CrateRule;
use crate::findings::Rule;

const WORKSPACE: &str = r#"
[workspace]
members = ["crates/lavashell-core", "crates/middle"]

[workspace.dependencies]
ash = "=0.38.0"
loader = "=1.0.0"
lavashell-core = { path = "crates/lavashell-core" }
middle = { path = "crates/middle" }
"#;

const LOCKFILE: &str = r#"
version = 4

[[package]]
name = "ash"
version = "0.38.0"

[[package]]
name = "lavashell-core"
version = "0.1.0"
dependencies = ["ash"]

[[package]]
name = "loader"
version = "1.0.0"
dependencies = ["ash 0.38.0"]

[[package]]
name = "middle"
version = "0.1.0"
"#;

fn manifest(name: &str, tables: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion.workspace = true\nedition.workspace = true\n\n\
         [lints]\nworkspace = true\n\n{tables}"
    )
}

fn findings(core: &str, middle: &str) -> Result<Vec<(String, Rule)>, Failure> {
    let core = manifest("lavashell-core", core);
    let middle = manifest("middle", middle);
    Ok(findings_of(
        &[
            ("Cargo.toml", WORKSPACE),
            ("Cargo.lock", LOCKFILE),
            ("crates/lavashell-core/Cargo.toml", &core),
            ("crates/middle/Cargo.toml", &middle),
            (CARGO_WORKSPACES, ROOT_WORKSPACE_LISTING),
        ],
        crate_report_of_every_workspace,
    )?
    .into_iter()
    .map(|(path, _, rule)| (path, rule))
    .collect())
}

fn core_finding(through: &str) -> Vec<(String, Rule)> {
    let rule = Rule::Crate(CrateRule::VulkanDependency {
        vulkan: "ash".to_owned(),
        through: through.to_owned(),
    });
    vec![("crates/lavashell-core/Cargo.toml".to_owned(), rule)]
}

#[test]
fn a_dependency_on_vulkan_in_a_test_passes() -> Result<(), Failure> {
    let core = "[dev-dependencies]\nash.workspace = true\n";
    assert_eq!(findings(core, "")?, []);
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_with_a_direct_dependency() -> Result<(), Failure> {
    let core = "[dependencies]\nash.workspace = true\n";
    assert_eq!(findings(core, "")?, core_finding("ash"));
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_through_a_member() -> Result<(), Failure> {
    let core = "[dependencies]\nmiddle.workspace = true\n";
    let middle = "[target.'cfg(unix)'.build-dependencies]\nash.workspace = true\n";
    assert_eq!(findings(core, middle)?, core_finding("middle"));
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_through_a_locked_package() -> Result<(), Failure> {
    let core = "[dependencies]\nloader.workspace = true\n";
    assert_eq!(findings(core, "")?, core_finding("loader"));
    Ok(())
}
