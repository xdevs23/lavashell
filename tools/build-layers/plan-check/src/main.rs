// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod error;

use clap::Parser;
use error::Error;
use lavashell_cargo_model::contract::ProjectionContract;
use lavashell_cargo_model::projection::EngineProjection;
use lavashell_cargo_model::unit_graph::UnitGraph;
use serde::de::DeserializeOwned;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    contract: PathBuf,
    #[arg(long)]
    engine: PathBuf,
    #[arg(long)]
    consumer: PathBuf,
}

fn main() -> ExitCode {
    match run(&Arguments::parse()) {
        Ok(engine_units) => {
            println!("Verified {engine_units} compatible engine external units");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error} {}", error.next_step());
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &Arguments) -> Result<usize, Error> {
    let contract: ProjectionContract = read_json(&arguments.contract)?;
    let engine: EngineProjection = read_json(&arguments.engine)?;
    let consumer: UnitGraph = read_json(&arguments.consumer)?;
    let plan =
        lavashell_plan_check::verify(&engine.external_units, &consumer, &contract.engine_root)?;
    Ok(plan.engine_units)
}

fn read_json<Document: DeserializeOwned>(path: &Path) -> Result<Document, Error> {
    let file = File::open(path).map_err(|source| Error::Open {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_reader(BufReader::new(file)).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}
