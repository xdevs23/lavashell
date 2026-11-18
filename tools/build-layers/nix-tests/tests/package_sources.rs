// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;

const EXPRESSION: &str = "package-sources.nix";
const CLIPPY_CONFIGURATION: &str = "clippy.toml";
const INCLUDE_MACROS: [&str; 2] = ["include_str", "include_bytes"];
const RUST_EXTENSION: &str = "rs";

#[derive(Serialize)]
struct TreeArguments {
    root: PathBuf,
}

#[derive(Deserialize)]
struct PackageSource {
    source: PathBuf,
    members: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
enum SourceTestError {
    #[error(transparent)]
    Fixture(#[from] FixtureError),
    #[error(transparent)]
    Read(#[from] std::io::Error),
    #[error("{path} is not valid Rust: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: syn::Error,
    },
}

#[derive(Default)]
struct IncludedPaths(Vec<String>);

impl<'ast> Visit<'ast> for IncludedPaths {
    fn visit_macro(&mut self, called: &'ast syn::Macro) {
        if INCLUDE_MACROS.iter().any(|name| called.path.is_ident(name)) {
            self.0.extend(
                called
                    .parse_body::<syn::LitStr>()
                    .ok()
                    .map(|path| path.value()),
            );
        }
        syn::visit::visit_macro(self, called);
    }
}

fn rust_files(directory: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(rust_files(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == RUST_EXTENSION)
        {
            files.push(path);
        }
    }
    Ok(files)
}

fn included_paths(file: &Path) -> Result<Vec<String>, SourceTestError> {
    let parsed =
        syn::parse_file(&fs::read_to_string(file)?).map_err(|source| SourceTestError::Parse {
            path: file.to_owned(),
            source,
        })?;
    let mut included = IncludedPaths::default();
    included.visit_file(&parsed);
    Ok(included.0)
}

fn packaged_sources() -> Result<BTreeMap<String, PackageSource>, FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("repository");
    copy_repository(&root)?;
    evaluate(EXPRESSION, &TreeArguments { root })
}

#[test]
fn every_packaged_source_has_the_clippy_configuration() -> Result<(), FixtureError> {
    for (name, package) in packaged_sources()? {
        let configuration = package.source.join(CLIPPY_CONFIGURATION);
        assert!(
            configuration.exists(),
            "{name}: {}",
            configuration.display()
        );
    }
    Ok(())
}

#[test]
fn every_file_included_by_a_packaged_crate_is_in_its_source() -> Result<(), SourceTestError> {
    for (name, package) in &packaged_sources()? {
        for member in &package.members {
            for file in rust_files(&package.source.join(member))? {
                for path in included_paths(&file)? {
                    assert!(
                        file.with_file_name(&path).exists(),
                        "{name}: {} includes {path}, which its source lacks",
                        file.display()
                    );
                }
            }
        }
    }
    Ok(())
}
