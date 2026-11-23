// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CARGO_WORKSPACES, Failure, ROOT_WORKSPACE_LISTING, crate_report_of_every_workspace, findings_of,
};
use crate::crate_manifests::{CrateRule, DependencyTable};
use crate::findings::Rule;

const WORKSPACE: &str = r#"
[workspace]
members = ["crates/lavashell-core", "crates/lavashell-runtime", "crates/middle"]

[workspace.dependencies]
ash = "=0.38.0"
loader = "=1.0.0"
lavashell-core = { path = "crates/lavashell-core" }
lavashell-runtime = { path = "crates/lavashell-runtime" }
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
name = "lavashell-runtime"
version = "0.1.0"

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

#[derive(Clone, Copy, Default)]
struct MemberTables<'a> {
    core: &'a str,
    runtime: &'a str,
    middle: &'a str,
}

fn findings(tables: MemberTables<'_>) -> Result<Vec<(String, Rule)>, Failure> {
    let core = manifest("lavashell-core", tables.core);
    let runtime = manifest("lavashell-runtime", tables.runtime);
    let middle = manifest("middle", tables.middle);
    Ok(findings_of(
        &[
            ("Cargo.toml", WORKSPACE),
            ("Cargo.lock", LOCKFILE),
            ("crates/lavashell-core/Cargo.toml", &core),
            ("crates/lavashell-runtime/Cargo.toml", &runtime),
            ("crates/middle/Cargo.toml", &middle),
            (CARGO_WORKSPACES, ROOT_WORKSPACE_LISTING),
        ],
        crate_report_of_every_workspace,
    )?
    .into_iter()
    .map(|(path, _, rule)| (path, rule))
    .collect())
}

fn vulkan_finding(member: &str, table: DependencyTable, through: &str) -> Vec<(String, Rule)> {
    let rule = Rule::Crate(CrateRule::VulkanDependency {
        vulkan: "ash".to_owned(),
        through: through.to_owned(),
        table,
    });
    vec![(format!("crates/{member}/Cargo.toml"), rule)]
}

fn core_finding(through: &str) -> Vec<(String, Rule)> {
    vulkan_finding("lavashell-core", DependencyTable::Dependencies, through)
}

#[test]
fn a_crate_that_builds_without_vulkan_passes_with_vulkan_in_its_tests() -> Result<(), Failure> {
    let core = "[dev-dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        core,
        ..MemberTables::default()
    };
    assert_eq!(findings(tables)?, []);
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_with_a_direct_dependency() -> Result<(), Failure> {
    let core = "[dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        core,
        ..MemberTables::default()
    };
    assert_eq!(findings(tables)?, core_finding("ash"));
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_through_a_member() -> Result<(), Failure> {
    let core = "[dependencies]\nmiddle.workspace = true\n";
    let middle = "[target.'cfg(unix)'.build-dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        core,
        middle,
        ..MemberTables::default()
    };
    assert_eq!(findings(tables)?, core_finding("middle"));
    Ok(())
}

#[test]
fn a_crate_that_builds_without_vulkan_fails_through_a_locked_package() -> Result<(), Failure> {
    let core = "[dependencies]\nloader.workspace = true\n";
    let tables = MemberTables {
        core,
        ..MemberTables::default()
    };
    assert_eq!(findings(tables)?, core_finding("loader"));
    Ok(())
}

#[test]
fn a_crate_whose_tests_build_without_vulkan_fails_with_a_direct_dependency() -> Result<(), Failure>
{
    let runtime = "[dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        runtime,
        ..MemberTables::default()
    };
    let expected = vulkan_finding("lavashell-runtime", DependencyTable::Dependencies, "ash");
    assert_eq!(findings(tables)?, expected);
    Ok(())
}

#[test]
fn a_crate_whose_tests_build_without_vulkan_fails_with_a_direct_test_dependency()
-> Result<(), Failure> {
    let runtime = "[dev-dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        runtime,
        ..MemberTables::default()
    };
    let expected = vulkan_finding("lavashell-runtime", DependencyTable::DevDependencies, "ash");
    assert_eq!(findings(tables)?, expected);
    Ok(())
}

#[test]
fn a_crate_whose_tests_build_without_vulkan_fails_with_a_test_dependency_through_a_member()
-> Result<(), Failure> {
    let runtime = "[target.'cfg(unix)'.dev-dependencies]\nmiddle.workspace = true\n";
    let middle = "[dependencies]\nash.workspace = true\n";
    let tables = MemberTables {
        runtime,
        middle,
        ..MemberTables::default()
    };
    let expected = vulkan_finding(
        "lavashell-runtime",
        DependencyTable::DevDependencies,
        "middle",
    );
    assert_eq!(findings(tables)?, expected);
    Ok(())
}
