// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod error;
mod reuse_report;

use clap::Parser;
use error::Error;
use lavashell_cargo_model::build_message::{BuildMessage, CompilerArtifact};
use lavashell_plan_check::stub_layer_reuse::{
    CheckedBuild, RecompiledUnit, StubLayerRecord, recompiled_units,
};
use reuse_report::RecompileReport;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    build: PathBuf,
    #[arg(long = "stub-layer", required = true)]
    stub_layers: Vec<PathBuf>,
    #[arg(long, value_enum)]
    checked_build: CheckedBuild,
}

fn main() -> ExitCode {
    check_stub_layer_reuse(&Arguments::parse()).unwrap_or_else(|error| {
        eprintln!("{error} {}", error.next_step());
        ExitCode::FAILURE
    })
}

fn check_stub_layer_reuse(arguments: &Arguments) -> Result<ExitCode, Error> {
    let build = read_build_record(&arguments.build)?;
    let stub_layers = arguments
        .stub_layers
        .iter()
        .map(|path| {
            Ok(StubLayerRecord::new(
                path.clone(),
                read_build_record(path)?,
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let recompiled: Vec<RecompiledUnit<'_>> =
        recompiled_units(&build, &stub_layers, arguments.checked_build).collect();
    for unit in &recompiled {
        eprintln!("{}", RecompileReport(unit));
    }
    if recompiled.is_empty() {
        println!(
            "Reused the planned third-party units of {} stub layers",
            stub_layers.len()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn open(path: &Path) -> Result<BufReader<File>, Error> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|source| Error::Open {
            path: path.to_owned(),
            source,
        })
}

fn read_build_record(path: &Path) -> Result<Vec<CompilerArtifact>, Error> {
    open(path)?
        .lines()
        .map(|line| {
            let line = line.map_err(|source| Error::ReadLine {
                path: path.to_owned(),
                source,
            })?;
            compiler_artifact(&line).map_err(|source| Error::Read {
                path: path.to_owned(),
                source,
            })
        })
        .filter_map(Result::transpose)
        .collect()
}

fn compiler_artifact(line: &str) -> Result<Option<CompilerArtifact>, serde_json::Error> {
    match serde_json::from_str(line) {
        Ok(BuildMessage::CompilerArtifact(artifact)) => Ok(Some(*artifact)),
        Ok(BuildMessage::Other) => Ok(None),
        Err(text_line) if text_line.is_syntax() || text_line.is_eof() => Ok(None),
        Err(error) => Err(error),
    }
}
