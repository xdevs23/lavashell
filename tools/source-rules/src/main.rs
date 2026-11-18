// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod attributes;
mod checks;
mod comments;
mod copy_catalogs;
mod crate_manifests;
mod documents;
mod enum_value_lists;
mod error;
mod findings;
mod grammar;
mod languages;
mod metadata;
mod modules;
mod next_steps;
mod patch;
mod python_place;
mod renderer_test_fixtures;
mod repository_files;
mod rules;
mod safety_comments;
#[cfg(test)]
mod tests;

use checks::{repository_reports, workspace_reports};
use error::Error;
use lavashell_error_chain::message_chain;
use metadata::Workspace;
use next_steps::next_step;
use repository_files::read_repository_files;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}\n{}", message_chain(&error), next_step(&error));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Error> {
    let workspace = Workspace::read()?;
    let root = &workspace.workspace_root;
    let files = read_repository_files(root)?;
    let mut reports = repository_reports(&files, root, &workspace.crate_roots())?;
    reports.extend(workspace_reports(&files, root)?);
    let findings: Vec<_> = reports
        .iter()
        .flat_map(|(_, report)| &report.findings)
        .collect();
    for finding in &findings {
        let path = finding.path.strip_prefix(root).unwrap_or(&finding.path);
        match finding.line {
            Some(line) => println!("{}:{line}: {}", path.display(), finding.rule),
            None => println!("{}: {}", path.display(), finding.rule),
        }
    }
    if !findings.is_empty() {
        return Ok(ExitCode::FAILURE);
    }
    for (rules, report) in &reports {
        println!("Passed {rules} for {} files", report.checked);
    }
    Ok(ExitCode::SUCCESS)
}
