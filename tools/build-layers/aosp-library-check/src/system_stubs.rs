// SPDX-License-Identifier: GPL-3.0-only
use crate::api_level::ApiLevel;
use crate::elf_library::{ElfError, ElfLibrary};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Introductions {
    pub libraries: BTreeMap<String, ApiLevel>,
    pub symbols: BTreeMap<String, ApiLevel>,
}

#[derive(Debug, thiserror::Error)]
pub enum StubError {
    #[error("the system library directory {path} cannot be listed: {source}")]
    ListLevels {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the system library stub {path} cannot be read: {source}")]
    ReadStub {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the system library stub {path} is not a valid ELF library: {source}")]
    ParseStub {
        path: PathBuf,
        #[source]
        source: ElfError,
    },
}

impl Introductions {
    pub fn find(directory: &Path, needed: &[String]) -> Result<Self, StubError> {
        let mut found = Self::default();
        let file_names: Vec<&String> = needed
            .iter()
            .filter(|name| Path::new(name).file_name() == Some(name.as_ref()))
            .collect();
        for level in api_levels(directory)? {
            for name in &file_names {
                let stub = directory.join(level.to_string()).join(name);
                if stub.is_file() {
                    record_earliest(&mut found.libraries, (*name).clone(), level);
                    for symbol in read_exports(&stub)? {
                        record_earliest(&mut found.symbols, symbol, level);
                    }
                }
            }
        }
        Ok(found)
    }
}

fn record_earliest(levels: &mut BTreeMap<String, ApiLevel>, name: String, level: ApiLevel) {
    let earliest = levels.entry(name).or_insert(level);
    *earliest = (*earliest).min(level);
}

fn api_levels(directory: &Path) -> Result<Vec<ApiLevel>, StubError> {
    let listing_failed = |source| StubError::ListLevels {
        path: directory.to_owned(),
        source,
    };
    fs::read_dir(directory)
        .map_err(listing_failed)?
        .map(|entry| entry.map(|entry| entry.file_name()).map_err(listing_failed))
        .filter_map(|name| match name {
            Ok(name) => name.to_str()?.parse().ok().map(Ok),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn read_exports(stub: &Path) -> Result<BTreeSet<String>, StubError> {
    let data = fs::read(stub).map_err(|source| StubError::ReadStub {
        path: stub.to_owned(),
        source,
    })?;
    ElfLibrary::parse(&data)
        .map(|library| library.exports)
        .map_err(|source| StubError::ParseStub {
            path: stub.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests;
