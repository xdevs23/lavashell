// SPDX-License-Identifier: GPL-3.0-only
use crate::elf_library::ElfLibrary;
use crate::report::{InspectionError, LibraryDigest, Report};
use crate::system_stubs::Introductions;
use crate::verdict::{Expectation, Verdict};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[must_use]
pub fn inspect(library: PathBuf, system_libraries: PathBuf, expectation: &Expectation) -> Report {
    let contents = fs::read(&library);
    let sha256 = contents
        .as_ref()
        .ok()
        .map(|data| LibraryDigest(Sha256::digest(data).into()));
    let outcome = contents
        .map_err(|source| InspectionError::ReadLibrary {
            path: library.clone(),
            source,
        })
        .and_then(|data| verify(&library, &data, &system_libraries, expectation));
    Report {
        library,
        system_libraries,
        sha256,
        outcome,
    }
}

fn verify(
    path: &Path,
    data: &[u8],
    system_libraries: &Path,
    expectation: &Expectation,
) -> Result<Verdict, InspectionError> {
    let library = ElfLibrary::parse(data).map_err(|source| InspectionError::ParseLibrary {
        path: path.to_owned(),
        source,
    })?;
    let introduced = Introductions::find(system_libraries, &library.needed)?;
    Ok(Verdict::of(&library, expectation, &introduced))
}

#[cfg(test)]
mod tests;
