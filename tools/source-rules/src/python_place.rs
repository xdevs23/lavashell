// SPDX-License-Identifier: GPL-3.0-only
use crate::findings::{Finding, Report, Rule};
use crate::languages::PYTHON;
use crate::repository_files::{FileKind, RepositoryFile};
use std::path::Path;

pub const PYTHON_DIRECTORY: &str = "nix";

pub fn python_place_report(files: &[RepositoryFile], python_directory: &Path) -> Report {
    let python_files: Vec<_> = files
        .iter()
        .filter(|file| {
            matches!(&file.kind, Some(FileKind::Source { language, .. }) if language.is(&PYTHON))
        })
        .collect();
    Report {
        checked: python_files.len(),
        findings: python_files
            .into_iter()
            .filter(|file| file.path.parent() != Some(python_directory))
            .map(|file| Finding {
                path: file.path.clone(),
                line: None,
                rule: Rule::PythonOutsidePlace {
                    directory: PYTHON_DIRECTORY,
                },
            })
            .collect(),
    }
}
