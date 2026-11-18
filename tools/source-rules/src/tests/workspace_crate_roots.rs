// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings_of};
use crate::checks::rust_report;
use crate::findings::Rule;
use crate::metadata::{Workspace, crate_roots_of_every_workspace, nested_workspace_directories};
use crate::repository_files::read_repository_files;

const WORKSPACE: &str = "[workspace]\nmembers = [\"member\"]\nresolver = \"3\"\n";

const MEMBER: &str = "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

#[test]
fn a_crate_root_of_a_nested_workspace_follows_the_rust_rules() -> Result<(), Failure> {
    let files = [
        ("Cargo.toml", WORKSPACE),
        ("member/Cargo.toml", MEMBER),
        ("member/src/lib.rs", "#![forbid(unsafe_code)]\n"),
        ("tools/nested/Cargo.toml", WORKSPACE),
        ("tools/nested/member/Cargo.toml", MEMBER),
        ("tools/nested/member/src/lib.rs", "fn call() {}\n"),
    ];
    let findings = findings_of(&files, |root| {
        let files = read_repository_files(root)?;
        let nested_directories = nested_workspace_directories(&files, root)?;
        let crate_roots =
            crate_roots_of_every_workspace(&Workspace::read(root)?, &nested_directories)?;
        rust_report(&files, &crate_roots)
    })?;
    assert_eq!(
        findings,
        [(
            "tools/nested/member/src/lib.rs".to_owned(),
            None,
            Rule::MissingForbid
        )]
    );
    Ok(())
}
