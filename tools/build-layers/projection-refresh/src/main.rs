// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod activations;
mod canonical;
mod cargo;
mod concurrency;
mod error;
mod files;
mod projection_start;
mod refresh;
mod seed;
mod stage;

use crate::cargo::Cargo;
use crate::error::Error;
use crate::files::read_json;
use crate::refresh::{PackageEntry, Toolkit, package_name, refresh_projections};
use crate::seed::Seeds;
use crate::stage::publish_staged_refresh;
use clap::Parser;
use lavashell_cargo_model::contract::ProjectionContract;
use lavashell_command_output::nix::NixEvaluator;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const CONTRACT: &str = "nix/projection-contract.json";

#[derive(Parser)]
struct Arguments {
    #[arg(long = "package")]
    packages: Vec<String>,
}

fn main() -> ExitCode {
    match run(&Arguments::parse().packages) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error} {}", error.next_step());
            ExitCode::FAILURE
        }
    }
}

fn example_directories(root: &Path) -> Result<Vec<String>, Error> {
    let examples = root.join("examples");
    let entries = fs::read_dir(&examples).map_err(|source| Error::ListDirectory {
        path: examples.clone(),
        source,
    })?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ListDirectory {
            path: examples.clone(),
            source,
        })?;
        if entry.path().join("src/main.rs").is_file() {
            directories.push(format!("examples/{}", entry.file_name().to_string_lossy()));
        }
    }
    directories.sort();
    Ok(directories)
}

fn package_entries(
    root: &Path,
    contract: &ProjectionContract,
    selected: &[String],
) -> Result<Vec<PackageEntry>, Error> {
    let entries = example_directories(root)?
        .into_iter()
        .chain(contract.packaged_crates.values().cloned())
        .map(|directory| {
            package_name(&root.join(&directory)).map(|package| PackageEntry { directory, package })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if selected.is_empty() {
        return Ok(entries);
    }
    let native = package_name(&root.join(&contract.native_directory))?;
    let unknown: Vec<String> = selected
        .iter()
        .filter(|name| **name != native && !entries.iter().any(|entry| entry.package == **name))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(Error::UnknownPackages { packages: unknown });
    }
    Ok(entries
        .into_iter()
        .filter(|entry| selected.contains(&entry.package))
        .collect())
}

fn run(packages: &[String]) -> Result<(), Error> {
    let root = std::env::current_dir().map_err(|source| Error::ReadFile {
        path: PathBuf::from("."),
        source,
    })?;
    if !root.join(CONTRACT).is_file() {
        return Err(Error::NotRepositoryRoot { directory: root });
    }
    let contract: ProjectionContract = read_json(&root.join(CONTRACT))?;
    let entries = package_entries(&root, &contract, packages)?;
    let nix = NixEvaluator::from_environment()?;
    let cargo = Cargo {
        target: contract.target.clone(),
        profile: contract.profile.clone(),
    };
    let refreshed = publish_staged_refresh(&root.join(&contract.projection_directory), |staged| {
        let toolkit = Toolkit {
            cargo: &cargo,
            seeds: Seeds {
                nix: &nix,
                root: &root,
                projections: staged,
                contract: &contract,
            },
            contract: &contract,
            root: &root,
        };
        refresh_projections(&toolkit, &entries)
    })?;
    for refreshed in refreshed {
        if let Some(unresolved) = refreshed.unresolved_stored_crate {
            println!(
                "{}: the stored projection lists {} {}, which Cargo.lock no longer resolves, so the refresh started from Cargo.lock",
                refreshed.package, unresolved.name, unresolved.version
            );
        }
        if let Some(units) = refreshed.external_units {
            println!("{}: {units} matching external units", refreshed.package);
        }
    }
    println!("Projection locks refreshed; no compilation performed");
    Ok(())
}
