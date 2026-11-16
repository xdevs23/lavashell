// SPDX-License-Identifier: GPL-3.0-only
use super::{Introductions, StubError};
use crate::api_level::ApiLevel;
use crate::synthetic_elf::{SyntheticElfError, SyntheticLibrary, SyntheticSymbol};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Synthetic(#[from] SyntheticElfError),
    #[error(transparent)]
    Stub(#[from] StubError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

fn write_stub(
    directory: &Path,
    level: &str,
    name: &str,
    exports: &[&'static str],
) -> Result<(), TestError> {
    let level_directory = directory.join(level);
    fs::create_dir_all(&level_directory)?;
    let symbols = exports
        .iter()
        .copied()
        .map(SyntheticSymbol::export)
        .collect();
    fs::write(
        level_directory.join(name),
        SyntheticLibrary::shared_object(Vec::new(), symbols).encode()?,
    )?;
    Ok(())
}

fn levels(entries: &[(&str, u32)]) -> BTreeMap<String, ApiLevel> {
    entries
        .iter()
        .map(|(name, level)| ((*name).to_owned(), ApiLevel(*level)))
        .collect()
}

#[test]
fn records_the_first_api_level_of_each_needed_library_and_symbol() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    write_stub(directory.path(), "24", "libc.so", &["malloc", "getrandom"])?;
    write_stub(directory.path(), "21", "libc.so", &["malloc"])?;
    write_stub(directory.path(), "28", "liblog.so", &["write_log_message"])?;
    write_stub(directory.path(), "28", "libunused.so", &["unused"])?;
    let found = Introductions::find(
        directory.path(),
        &["libc.so".to_owned(), "liblog.so".to_owned()],
    )?;
    assert_eq!(
        found.libraries,
        levels(&[("libc.so", 21), ("liblog.so", 28)])
    );
    assert_eq!(
        found.symbols,
        levels(&[("write_log_message", 28), ("getrandom", 24), ("malloc", 21)])
    );
    Ok(())
}

#[test]
fn skips_entries_that_are_not_api_levels_and_names_that_are_paths() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    write_stub(directory.path(), "latest", "libc.so", &["malloc"])?;
    write_stub(directory.path(), "+21", "libc.so", &["malloc"])?;
    write_stub(directory.path(), "21", "libc.so", &["malloc"])?;
    let found = Introductions::find(directory.path(), &["../21/libc.so".to_owned()])?;
    assert_eq!(found, Introductions::default());
    Ok(())
}

#[test]
fn reports_a_directory_that_cannot_be_listed() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing");
    assert!(matches!(
        Introductions::find(&missing, &["libc.so".to_owned()]),
        Err(StubError::ListLevels { path, .. }) if path == missing
    ));
    Ok(())
}
