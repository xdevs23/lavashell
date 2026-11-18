// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CARGO_WORKSPACES, Failure, ROOT_WORKSPACE_LISTING, crate_report_of_every_workspace, findings_of,
};
use crate::crate_manifests::{CrateRule, InheritedTable};
use crate::error::Error;
use crate::findings::Rule;

const WORKSPACE: &str = r#"
[workspace]
members = ["crates/member"]

[workspace.dependencies]
exact = "=1.0.0"
member = { path = "crates/member" }

[workspace.package]
version = "0.1.0"

[workspace.lints.rust]
unused_results = "deny"
"#;

const MEMBER: &str = "
[package]
name = \"member\"
version.workspace = true
edition.workspace = true

[lints]
workspace = true

[dependencies]
exact.workspace = true
";

const LOCKFILE: &str = r#"
version = 4

[[package]]
name = "member"
version = "0.1.0"
"#;

const NESTED_MANIFEST: &str = "tools/nested/Cargo.toml";

const NESTED_WORKSPACE_LISTING: &str = "inherited-tables = [\"package\", \"lints\"]\n\n\
    [directories]\nmain = \".\"\nnested = \"tools/nested\"\n";

fn findings(files: &[(&str, &str)]) -> Result<Vec<(String, Rule)>, Failure> {
    Ok(findings_of(files, crate_report_of_every_workspace)?
        .into_iter()
        .map(|(path, _, rule)| (path, rule))
        .collect())
}

fn root_files<'a>(workspace: &'a str, member: &'a str) -> [(&'a str, &'a str); 4] {
    root_files_with_listing(workspace, member, ROOT_WORKSPACE_LISTING)
}

fn root_files_with_listing<'a>(
    workspace: &'a str,
    member: &'a str,
    listing: &'a str,
) -> [(&'a str, &'a str); 4] {
    [
        ("Cargo.toml", workspace),
        ("Cargo.lock", LOCKFILE),
        ("crates/member/Cargo.toml", member),
        (CARGO_WORKSPACES, listing),
    ]
}

fn nested_files<'a>(nested: &'a str, member: &'a str) -> [(&'a str, &'a str); 7] {
    nested_files_with_listing(nested, member, NESTED_WORKSPACE_LISTING)
}

fn nested_files_with_listing<'a>(
    nested: &'a str,
    member: &'a str,
    listing: &'a str,
) -> [(&'a str, &'a str); 7] {
    [
        ("Cargo.toml", WORKSPACE),
        ("Cargo.lock", LOCKFILE),
        ("crates/member/Cargo.toml", MEMBER),
        (NESTED_MANIFEST, nested),
        ("tools/nested/Cargo.lock", LOCKFILE),
        ("tools/nested/crates/member/Cargo.toml", member),
        (CARGO_WORKSPACES, listing),
    ]
}

fn workspace_finding(manifest: &str, rule: Rule) -> Vec<(String, Rule)> {
    vec![(manifest.to_owned(), rule)]
}

fn member_finding(rule: Rule) -> Vec<(String, Rule)> {
    vec![("crates/member/Cargo.toml".to_owned(), rule)]
}

fn different_table(table: InheritedTable) -> Rule {
    Rule::Crate(CrateRule::DifferentWorkspaceTable { table })
}

fn inexact() -> Rule {
    Rule::Crate(CrateRule::InexactWorkspaceDependency {
        name: "exact".to_owned(),
    })
}

#[test]
fn a_conforming_workspace_passes() -> Result<(), Failure> {
    assert_eq!(findings(&root_files(WORKSPACE, MEMBER))?, []);
    Ok(())
}

#[test]
fn a_table_with_an_exact_version_passes() -> Result<(), Failure> {
    let exact = WORKSPACE.replace(r#""=1.0.0""#, r#"{ version = "=1.0.0", features = [] }"#);
    assert_eq!(findings(&root_files(&exact, MEMBER))?, []);
    Ok(())
}

#[test]
fn a_dependency_range_fails() -> Result<(), Failure> {
    let ranged = WORKSPACE.replace("=1.0.0", "1.0.0");
    assert_eq!(
        findings(&root_files(&ranged, MEMBER))?,
        workspace_finding("Cargo.toml", inexact())
    );
    Ok(())
}

#[test]
fn a_table_with_a_range_fails() -> Result<(), Failure> {
    let ranged = WORKSPACE.replace(r#""=1.0.0""#, r#"{ version = "^1.0", features = [] }"#);
    assert_eq!(
        findings(&root_files(&ranged, MEMBER))?,
        workspace_finding("Cargo.toml", inexact())
    );
    Ok(())
}

#[test]
fn a_member_without_a_path_entry_fails() -> Result<(), Failure> {
    let unlisted = WORKSPACE.replace("member = { path = \"crates/member\" }\n", "");
    let rule = Rule::Crate(CrateRule::UnlistedMember {
        member: "crates/member".to_owned(),
    });
    assert_eq!(
        findings(&root_files(&unlisted, MEMBER))?,
        workspace_finding("Cargo.toml", rule)
    );
    Ok(())
}

#[test]
fn a_member_dependency_of_its_own_fails() -> Result<(), Failure> {
    let own = MEMBER.replace("exact.workspace = true", "exact = \"=1.0.0\"");
    let rule = Rule::Crate(CrateRule::OwnDependency {
        name: "exact".to_owned(),
    });
    assert_eq!(
        findings(&root_files(WORKSPACE, &own))?,
        member_finding(rule)
    );
    Ok(())
}

#[test]
fn a_target_dependency_of_its_own_fails() -> Result<(), Failure> {
    let own = format!("{MEMBER}\n[target.'cfg(unix)'.dependencies]\nother = \"=2.0.0\"\n");
    let rule = Rule::Crate(CrateRule::OwnDependency {
        name: "other".to_owned(),
    });
    assert_eq!(
        findings(&root_files(WORKSPACE, &own))?,
        member_finding(rule)
    );
    Ok(())
}

#[test]
fn a_member_without_workspace_lints_fails() -> Result<(), Failure> {
    let without_lints = MEMBER.replace("[lints]\nworkspace = true\n", "");
    assert_eq!(
        findings(&root_files(WORKSPACE, &without_lints))?,
        member_finding(Rule::Crate(CrateRule::OwnLints))
    );
    Ok(())
}

#[test]
fn lints_of_its_own_next_to_the_workspace_lints_fail() -> Result<(), Failure> {
    let extended = MEMBER.replace(
        "[lints]\nworkspace = true\n",
        "[lints]\nworkspace = true\n\n[lints.rust]\nunused = \"deny\"\n",
    );
    assert_eq!(
        findings(&root_files(WORKSPACE, &extended))?,
        member_finding(Rule::Crate(CrateRule::OwnLints))
    );
    Ok(())
}

#[test]
fn a_version_and_an_edition_of_its_own_fail() -> Result<(), Failure> {
    let own = MEMBER
        .replace("version.workspace = true", "version = \"0.1.0\"")
        .replace("edition.workspace = true", "edition = \"2024\"");
    let manifest = || "crates/member/Cargo.toml".to_owned();
    assert_eq!(
        findings(&root_files(WORKSPACE, &own))?,
        [
            (manifest(), Rule::Crate(CrateRule::OwnVersion)),
            (manifest(), Rule::Crate(CrateRule::OwnEdition))
        ]
    );
    Ok(())
}

#[test]
fn a_nested_workspace_with_the_tables_of_the_root_passes() -> Result<(), Failure> {
    assert_eq!(findings(&nested_files(WORKSPACE, MEMBER))?, []);
    Ok(())
}

#[test]
fn a_nested_workspace_without_the_tables_of_the_root_fails() -> Result<(), Failure> {
    let bare = WORKSPACE
        .replace("[workspace.package]\nversion = \"0.1.0\"\n", "")
        .replace("[workspace.lints.rust]\nunused_results = \"deny\"\n", "");
    let manifest = || NESTED_MANIFEST.to_owned();
    assert_eq!(
        findings(&nested_files(&bare, MEMBER))?,
        [
            (manifest(), different_table(InheritedTable::Package)),
            (manifest(), different_table(InheritedTable::Lints))
        ]
    );
    Ok(())
}

#[test]
fn a_nested_workspace_without_dependencies_reports_its_unlisted_member() -> Result<(), Failure> {
    let without_dependencies = WORKSPACE.replace(
        "[workspace.dependencies]\nexact = \"=1.0.0\"\nmember = { path = \"crates/member\" }\n",
        "",
    );
    let rule = Rule::Crate(CrateRule::UnlistedMember {
        member: "crates/member".to_owned(),
    });
    assert_eq!(
        findings(&nested_files(&without_dependencies, MEMBER))?,
        workspace_finding(NESTED_MANIFEST, rule)
    );
    Ok(())
}

#[test]
fn nested_workspace_lints_that_differ_from_the_root_fail() -> Result<(), Failure> {
    let changed = WORKSPACE.replace("unused_results", "unused_imports");
    assert_eq!(
        findings(&nested_files(&changed, MEMBER))?,
        workspace_finding(NESTED_MANIFEST, different_table(InheritedTable::Lints))
    );
    Ok(())
}

#[test]
fn a_nested_workspace_package_that_differs_from_the_root_fails() -> Result<(), Failure> {
    let changed = WORKSPACE.replace("version = \"0.1.0\"", "version = \"0.2.0\"");
    assert_eq!(
        findings(&nested_files(&changed, MEMBER))?,
        workspace_finding(NESTED_MANIFEST, different_table(InheritedTable::Package))
    );
    Ok(())
}

#[test]
fn a_member_of_a_nested_workspace_follows_the_crate_rules() -> Result<(), Failure> {
    let own = MEMBER.replace("edition.workspace = true", "edition = \"2024\"");
    assert_eq!(
        findings(&nested_files(WORKSPACE, &own))?,
        [(
            "tools/nested/crates/member/Cargo.toml".to_owned(),
            Rule::Crate(CrateRule::OwnEdition)
        )]
    );
    Ok(())
}

#[test]
fn a_cargo_workspace_missing_from_the_listing_fails() -> Result<(), Failure> {
    let nested = [
        (NESTED_MANIFEST, WORKSPACE),
        ("tools/nested/Cargo.lock", LOCKFILE),
        ("tools/nested/crates/member/Cargo.toml", MEMBER),
    ];
    let rule = Rule::Crate(CrateRule::UnlistedWorkspace {
        listing: CARGO_WORKSPACES,
    });
    assert_eq!(
        findings(&[&root_files(WORKSPACE, MEMBER)[..], &nested].concat())?,
        workspace_finding(NESTED_MANIFEST, rule)
    );
    Ok(())
}

#[test]
fn a_listed_directory_without_a_cargo_workspace_fails() -> Result<(), Failure> {
    let package = "[package]\nname = \"nested\"\n";
    let rule = Rule::Crate(CrateRule::ListedWithoutWorkspace {
        directory: "tools/nested".to_owned(),
    });
    assert_eq!(
        findings(&nested_files(package, MEMBER))?,
        workspace_finding(CARGO_WORKSPACES, rule)
    );
    Ok(())
}

#[test]
fn a_table_that_nested_workspaces_do_not_copy_is_reported_and_the_others_still_apply()
-> Result<(), Failure> {
    let listing = "inherited-tables = [\"profile\", \"package\"]\n\n\
        [directories]\nmain = \".\"\nnested = \"tools/nested\"\n";
    let changed = WORKSPACE.replace("version = \"0.1.0\"", "version = \"0.2.0\"");
    let unknown = Rule::Crate(CrateRule::UnknownInheritedTable {
        name: "profile".to_owned(),
    });
    assert_eq!(
        findings(&nested_files_with_listing(&changed, MEMBER, listing))?,
        [
            (
                NESTED_MANIFEST.to_owned(),
                different_table(InheritedTable::Package)
            ),
            (CARGO_WORKSPACES.to_owned(), unknown),
        ]
    );
    Ok(())
}

#[test]
fn a_workspace_listing_that_is_not_toml_fails_the_check() {
    let files = root_files_with_listing(WORKSPACE, MEMBER, "inherited-tables = [");
    assert!(matches!(
        findings(&files),
        Err(Failure::Check(Error::ReadWorkspaceList { path, .. })) if path.ends_with(CARGO_WORKSPACES)
    ));
}

#[test]
fn a_missing_workspace_listing_fails_the_check() {
    let files = &root_files(WORKSPACE, MEMBER)[..3];
    assert!(matches!(
        findings(files),
        Err(Failure::Check(Error::ReadSource { path, .. })) if path.ends_with(CARGO_WORKSPACES)
    ));
}
