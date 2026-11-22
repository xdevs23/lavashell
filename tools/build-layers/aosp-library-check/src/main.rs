// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod api_level;
mod architecture;
mod elf_library;
mod inspection;
mod report;
#[cfg(test)]
mod synthetic_elf;
mod system_stubs;
mod verdict;

use api_level::{ApiLevel, ApiLevelMatch};
use architecture::Architecture;
use clap::Parser;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::process::ExitCode;
use verdict::Expectation;

#[derive(Parser)]
struct Arguments {
    library: PathBuf,
    #[arg(
        long,
        help = "NDK directory of the system stubs of the ABI, with one subdirectory per API level"
    )]
    system_libraries: PathBuf,
    #[arg(long, help = "API level that the library was built for")]
    api_level: ApiLevel,
    #[arg(
        long,
        value_enum,
        default_value_t = ApiLevelMatch::Exact,
        help = "Whether the API level must equal or may exceed the level of the newest import"
    )]
    api_level_match: ApiLevelMatch,
    #[arg(long, help = "Page size in bytes that segments align to")]
    page_size: NonZeroU64,
    #[arg(long, value_enum, help = "Architecture of the library")]
    arch: Architecture,
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let expectation = Expectation {
        architecture: arguments.arch,
        page_size: arguments.page_size,
        api_level: arguments.api_level,
        api_level_match: arguments.api_level_match,
    };
    let report = inspection::inspect(arguments.library, arguments.system_libraries, &expectation);
    match serde_json::to_string_pretty(&report) {
        Ok(document) => {
            println!("{document}");
            report.status().exit_code()
        }
        Err(error) => {
            eprintln!("the report cannot be encoded as JSON: {error}");
            report::Status::Error.exit_code()
        }
    }
}
