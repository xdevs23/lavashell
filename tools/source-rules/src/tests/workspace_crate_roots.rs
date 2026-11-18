// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings_of};
use crate::checks::rust_report;
use crate::error::Error;
use crate::findings::{Report, Rule};
use crate::metadata::{Workspace, Workspaces, nested_workspace_directories};
use crate::repository_files::read_repository_files;
use std::path::Path;

const WORKSPACE: &str = "[workspace]\nmembers = [\"member\"]\nresolver = \"3\"\n";

const MEMBER: &str = "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

fn rust_report_of_every_workspace(root: &Path) -> Result<Report, Error> {
    let files = read_repository_files(root)?;
    let nested_directories = nested_workspace_directories(&files, root)?;
    let root_workspace = Workspace::read(root)?;
    let workspaces = Workspaces::read(&root_workspace, &nested_directories)?;
    rust_report(
        &files,
        &workspaces.crate_roots(),
        &workspaces.extern_preludes()?,
    )
}

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
    assert_eq!(
        findings_of(&files, rust_report_of_every_workspace)?,
        [(
            "tools/nested/member/src/lib.rs".to_owned(),
            None,
            Rule::MissingForbid
        )]
    );
    Ok(())
}

#[test]
fn a_list_of_every_value_of_an_enum_of_another_crate_of_a_nested_workspace_is_reported()
-> Result<(), Failure> {
    let files = [
        ("Cargo.toml", WORKSPACE),
        ("member/Cargo.toml", MEMBER),
        ("member/src/lib.rs", "#![forbid(unsafe_code)]\n"),
        (
            "tools/nested/Cargo.toml",
            "[workspace]\nmembers = [\"core\", \"shell\"]\nresolver = \"3\"\n",
        ),
        (
            "tools/nested/core/Cargo.toml",
            "[package]\nname = \"edge-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        (
            "tools/nested/core/src/lib.rs",
            "#![forbid(unsafe_code)]\npub enum Edge { Top, Bottom }\n",
        ),
        (
            "tools/nested/shell/Cargo.toml",
            concat!(
                "[package]\nname = \"shell\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
                "[dependencies]\nedge-core = { path = \"../core\" }\n",
            ),
        ),
        (
            "tools/nested/shell/src/lib.rs",
            concat!(
                "#![forbid(unsafe_code)]\n",
                "fn run() {\n",
                "    for edge in [edge_core::Edge::Top, edge_core::Edge::Bottom] {}\n",
                "}\n",
            ),
        ),
    ];
    let enum_name = "Edge".to_owned();
    assert_eq!(
        findings_of(&files, rust_report_of_every_workspace)?,
        [(
            "tools/nested/shell/src/lib.rs".to_owned(),
            Some(3),
            Rule::HandWrittenEnumValues { enum_name }
        )]
    );
    Ok(())
}

#[test]
fn a_crate_name_refers_to_the_path_dependency_of_the_crate_that_uses_it() -> Result<(), Failure> {
    let core =
        |variants: &str| format!("#![forbid(unsafe_code)]\npub enum Edge {{ {variants} }}\n");
    let package = |name: &str| {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
             [dependencies]\nedge-core = {{ path = \"../core\" }}\n"
        )
    };
    let lists = "#![forbid(unsafe_code)]\nfn run() {\n    let _vertical = [edge_core::Edge::Top, edge_core::Edge::Bottom];\n    let _horizontal = [edge_core::Edge::Left, edge_core::Edge::Right];\n}\n";
    let core_manifest =
        "[package]\nname = \"edge-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";
    let nested_workspace = "[workspace]\nmembers = [\"core\", \"shell\"]\nresolver = \"3\"\n";
    let (vertical, horizontal) = (core("Top, Bottom"), core("Left, Right"));
    let (root_shell, nested_shell) = (package("shell"), package("nested-shell"));
    let files = [
        ("Cargo.toml", nested_workspace),
        ("core/Cargo.toml", core_manifest),
        ("core/src/lib.rs", &vertical),
        ("shell/Cargo.toml", &root_shell),
        ("shell/src/lib.rs", lists),
        ("tools/nested/Cargo.toml", nested_workspace),
        ("tools/nested/core/Cargo.toml", core_manifest),
        ("tools/nested/core/src/lib.rs", &horizontal),
        ("tools/nested/shell/Cargo.toml", &nested_shell),
        ("tools/nested/shell/src/lib.rs", lists),
    ];
    let hand_written = |path: &str, line| {
        let enum_name = "Edge".to_owned();
        (
            path.to_owned(),
            Some(line),
            Rule::HandWrittenEnumValues { enum_name },
        )
    };
    assert_eq!(
        findings_of(&files, rust_report_of_every_workspace)?,
        [
            hand_written("shell/src/lib.rs", 3),
            hand_written("tools/nested/shell/src/lib.rs", 4),
        ]
    );
    Ok(())
}
