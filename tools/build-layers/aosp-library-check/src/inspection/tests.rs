// SPDX-License-Identifier: GPL-3.0-only
use super::inspect;
use crate::api_level::ApiLevel;
use crate::architecture::Architecture;
use crate::report::{InspectionError, Status};
use crate::synthetic_elf::{PAGE_SIZE, SyntheticElfError, SyntheticLibrary, SyntheticSymbol};
use crate::verdict::{Expectation, Problem};
use serde_json::{Value, json};
use std::fs;
use std::io;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Synthetic(#[from] SyntheticElfError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn expectation(api_level: u32) -> Expectation {
    Expectation {
        architecture: Architecture::X86_64,
        page_size: PAGE_SIZE,
        api_level: ApiLevel(api_level),
    }
}

#[test]
fn reports_a_passing_library_with_its_digest() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let stubs = directory.path().join("stubs");
    fs::create_dir_all(stubs.join("26"))?;
    fs::write(
        stubs.join("26/libc.so"),
        SyntheticLibrary::shared_object(Vec::new(), vec![SyntheticSymbol::export("malloc")])
            .encode()?,
    )?;
    let library = directory.path().join("libround_trip.so");
    fs::write(
        &library,
        SyntheticLibrary::shared_object(vec!["libc.so"], vec![SyntheticSymbol::import("malloc")])
            .encode()?,
    )?;
    let report = inspect(library, stubs, &expectation(26));
    assert_eq!(report.status(), Status::Pass);
    let document = serde_json::to_value(&report)?;
    assert_eq!(document["status"], serde_json::to_value(Status::Pass)?);
    assert_eq!(document["next_step"], Value::Null);
    assert_eq!(document["verdict"]["problems"], json!([]));
    assert_eq!(
        document["verdict"]["architecture"],
        json!(format!("{:?}", object::elf::EM_X86_64))
    );
    assert_eq!(
        document["sha256"].as_str().map(str::len),
        Some(64),
        "a SHA-256 digest is 64 hexadecimal digits"
    );
    Ok(())
}

#[test]
fn reports_a_failing_library_with_messages_for_its_problems() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let stubs = directory.path().join("stubs");
    fs::create_dir_all(&stubs)?;
    let library = directory.path().join("libround_trip.so");
    fs::write(
        &library,
        SyntheticLibrary::shared_object(Vec::new(), Vec::new()).encode()?,
    )?;
    let report = inspect(library, stubs, &expectation(26));
    assert_eq!(report.status(), Status::Fail);
    let document = serde_json::to_value(&report)?;
    let expected_problem = Problem::ApiLevelMismatch {
        declared: ApiLevel(26),
        required: None,
    };
    assert_eq!(
        document["verdict"]["problems"],
        json!([expected_problem.to_string()])
    );
    assert!(document["next_step"].is_string());
    Ok(())
}

#[test]
fn reports_a_library_that_cannot_be_read_as_an_error_without_a_digest() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let report = inspect(
        directory.path().join("missing.so"),
        directory.path().to_owned(),
        &expectation(26),
    );
    assert_eq!(report.status(), Status::Error);
    assert!(matches!(
        report.outcome,
        Err(InspectionError::ReadLibrary { .. })
    ));
    let document = serde_json::to_value(&report)?;
    assert_eq!(document["sha256"], Value::Null);
    assert_eq!(document["verdict"], Value::Null);
    Ok(())
}
