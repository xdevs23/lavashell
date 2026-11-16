// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings_of};
use crate::crate_manifests::crate_report;
use crate::findings::Rule;

const WORKSPACE: &str = r#"
[workspace]
members = ["crates/member"]

[workspace.dependencies]
exact = "=1.0.0"
member = { path = "crates/member" }
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

fn findings(workspace: &str, member: &str) -> Result<Vec<(String, Rule)>, Failure> {
    Ok(findings_of(
        &[
            ("Cargo.toml", workspace),
            ("crates/member/Cargo.toml", member),
        ],
        crate_report,
    )?
    .into_iter()
    .map(|(path, _, rule)| (path, rule))
    .collect())
}

fn workspace_finding(rule: Rule) -> Vec<(String, Rule)> {
    vec![("Cargo.toml".to_owned(), rule)]
}

fn member_finding(rule: Rule) -> Vec<(String, Rule)> {
    vec![("crates/member/Cargo.toml".to_owned(), rule)]
}

fn inexact() -> Rule {
    Rule::InexactWorkspaceDependency {
        name: "exact".to_owned(),
    }
}

#[test]
fn a_conforming_workspace_passes() -> Result<(), Failure> {
    assert_eq!(findings(WORKSPACE, MEMBER)?, []);
    Ok(())
}

#[test]
fn a_table_with_an_exact_version_passes() -> Result<(), Failure> {
    let exact = WORKSPACE.replace(r#""=1.0.0""#, r#"{ version = "=1.0.0", features = [] }"#);
    assert_eq!(findings(&exact, MEMBER)?, []);
    Ok(())
}

#[test]
fn a_dependency_range_fails() -> Result<(), Failure> {
    let ranged = WORKSPACE.replace("=1.0.0", "1.0.0");
    assert_eq!(findings(&ranged, MEMBER)?, workspace_finding(inexact()));
    Ok(())
}

#[test]
fn a_table_with_a_range_fails() -> Result<(), Failure> {
    let ranged = WORKSPACE.replace(r#""=1.0.0""#, r#"{ version = "^1.0", features = [] }"#);
    assert_eq!(findings(&ranged, MEMBER)?, workspace_finding(inexact()));
    Ok(())
}

#[test]
fn a_member_without_a_path_entry_fails() -> Result<(), Failure> {
    let unlisted = WORKSPACE.replace("member = { path = \"crates/member\" }\n", "");
    let rule = Rule::UnlistedMember {
        member: "crates/member".to_owned(),
    };
    assert_eq!(findings(&unlisted, MEMBER)?, workspace_finding(rule));
    Ok(())
}

#[test]
fn a_member_dependency_of_its_own_fails() -> Result<(), Failure> {
    let own = MEMBER.replace("exact.workspace = true", "exact = \"=1.0.0\"");
    let rule = Rule::OwnDependency {
        name: "exact".to_owned(),
    };
    assert_eq!(findings(WORKSPACE, &own)?, member_finding(rule));
    Ok(())
}

#[test]
fn a_target_dependency_of_its_own_fails() -> Result<(), Failure> {
    let own = format!("{MEMBER}\n[target.'cfg(unix)'.dependencies]\nother = \"=2.0.0\"\n");
    let rule = Rule::OwnDependency {
        name: "other".to_owned(),
    };
    assert_eq!(findings(WORKSPACE, &own)?, member_finding(rule));
    Ok(())
}

#[test]
fn a_member_without_workspace_lints_fails() -> Result<(), Failure> {
    let without_lints = MEMBER.replace("[lints]\nworkspace = true\n", "");
    assert_eq!(
        findings(WORKSPACE, &without_lints)?,
        member_finding(Rule::OwnLints)
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
        findings(WORKSPACE, &extended)?,
        member_finding(Rule::OwnLints)
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
        findings(WORKSPACE, &own)?,
        [
            (manifest(), Rule::OwnVersion),
            (manifest(), Rule::OwnEdition)
        ]
    );
    Ok(())
}
