// SPDX-License-Identifier: GPL-3.0-only
//! Checks the Rust sources of the workspace in the current directory against the rules that neither rustc nor
//! clippy check, and lists every finding as `path:line: rule`.
#![forbid(unsafe_code)]

mod error;
mod metadata;
mod modules;
mod rules;
#[cfg(test)]
mod tests;

use error::Error;
use metadata::Workspace;
use modules::Crate;
use rules::{Finding, file_findings, missing_forbid};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> Result<ExitCode, Error> {
    let workspace = Workspace::read()?;
    let roots = workspace
        .packages
        .iter()
        .flat_map(|package| &package.targets)
        .map(|target| target.src_path.clone());
    let (findings, checked) = check(roots)?;
    for finding in &findings {
        let path = finding
            .path
            .strip_prefix(&workspace.workspace_root)
            .unwrap_or(&finding.path);
        println!("{}:{}: {}", path.display(), finding.line, finding.rule);
    }
    if findings.is_empty() {
        println!("Passed Rust source rules for {checked} files");
        return Ok(ExitCode::SUCCESS);
    }
    Ok(ExitCode::FAILURE)
}

/// The findings in the crates whose roots are `roots`, sorted by file and line, and the number of files checked.
fn check(roots: impl IntoIterator<Item = PathBuf>) -> Result<(Vec<Finding>, usize), Error> {
    let mut files = BTreeMap::new();
    let mut findings = Vec::new();
    for root in roots {
        let crate_files = Crate::read(&root)?;
        findings.extend(
            missing_forbid(&crate_files).map_err(|source| Error::ParseSource {
                path: root.clone(),
                source,
            })?,
        );
        let Crate { root, modules } = crate_files;
        files.extend(
            std::iter::once(root)
                .chain(modules)
                .map(|source| (source.path.clone(), source)),
        );
    }
    for source in files.values() {
        findings.extend(file_findings(source).map_err(|error| Error::ParseSource {
            path: source.path.clone(),
            source: error,
        })?);
    }
    findings.sort_by(|first, second| (&first.path, first.line).cmp(&(&second.path, second.line)));
    Ok((findings, files.len()))
}
