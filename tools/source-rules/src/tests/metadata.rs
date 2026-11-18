// SPDX-License-Identifier: GPL-3.0-only
use super::Failure;
use crate::error::Error;
use crate::metadata::{ExternPreludes, Workspace, Workspaces};
use std::path::Path;

fn extern_preludes_of(metadata: &str) -> Result<ExternPreludes, Failure> {
    let workspace: Workspace = serde_json::from_str(metadata)?;
    Ok(Workspaces::read(&workspace, &[])?.extern_preludes()?)
}

const CORE_LIBRARY: &str = "/repository/core/src/lib.rs";

#[test]
fn a_crate_finds_its_own_library_and_each_path_dependency_by_name_and_rename() -> Result<(), Failure>
{
    let preludes = extern_preludes_of(
        r#"{
            "workspace_root": "/repository",
            "packages": [
                {
                    "manifest_path": "/repository/core/Cargo.toml",
                    "targets": [
                        { "name": "edges", "kind": ["lib"], "src_path": "/repository/core/src/lib.rs" },
                        { "name": "edge-core", "kind": ["bin"], "src_path": "/repository/core/src/main.rs" }
                    ],
                    "dependencies": []
                },
                {
                    "manifest_path": "/repository/shell/Cargo.toml",
                    "targets": [
                        { "name": "shell", "kind": ["bin"], "src_path": "/repository/shell/src/main.rs" }
                    ],
                    "dependencies": [
                        { "rename": "core-edges", "path": "/repository/core" },
                        { "rename": "parsing", "path": null },
                        { "rename": null, "path": "/repository/core" }
                    ]
                }
            ]
        }"#,
    )?;
    let shell = Path::new("/repository/shell/src/main.rs");
    let core_binary = Path::new("/repository/core/src/main.rs");
    let core = Some(Path::new(CORE_LIBRARY));
    assert_eq!(preludes.library_root(shell, "core_edges"), core);
    assert_eq!(preludes.library_root(shell, "edges"), core);
    assert_eq!(preludes.library_root(shell, "parsing"), None);
    assert_eq!(preludes.library_root(core_binary, "edges"), core);
    assert_eq!(preludes.library_root(core_binary, "core_edges"), None);
    Ok(())
}

#[test]
fn one_crate_name_given_to_two_libraries_fails_the_check() {
    let preludes = extern_preludes_of(
        r#"{
            "workspace_root": "/repository",
            "packages": [
                {
                    "manifest_path": "/repository/core/Cargo.toml",
                    "targets": [{ "name": "edges", "kind": ["lib"], "src_path": "/repository/core/src/lib.rs" }],
                    "dependencies": []
                },
                {
                    "manifest_path": "/repository/sides/Cargo.toml",
                    "targets": [{ "name": "sides", "kind": ["lib"], "src_path": "/repository/sides/src/lib.rs" }],
                    "dependencies": []
                },
                {
                    "manifest_path": "/repository/shell/Cargo.toml",
                    "targets": [{ "name": "shell", "kind": ["bin"], "src_path": "/repository/shell/src/main.rs" }],
                    "dependencies": [
                        { "rename": "shapes", "path": "/repository/core" },
                        { "rename": "shapes", "path": "/repository/sides" }
                    ]
                }
            ]
        }"#,
    );
    assert!(matches!(
        preludes,
        Err(Failure::Check(Error::AmbiguousCrateName { name, .. })) if name == "shapes"
    ));
}
