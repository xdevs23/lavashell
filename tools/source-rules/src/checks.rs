// SPDX-License-Identifier: GPL-3.0-only
use crate::comments::comment_report;
use crate::copy_catalogs::copy_report;
use crate::crate_manifests::crate_report;
use crate::documents::{ALLOWED_DOCUMENT, document_report};
use crate::enum_value_lists::hand_written_value_lists;
use crate::error::Error;
use crate::findings::{Finding, Report};
use crate::languages::RUST;
use crate::modules::{Crate, Source, Sources};
use crate::python_place::{PYTHON_DIRECTORY, python_place_report};
use crate::renderer_test_fixtures::RENDERER_TEST_RULES;
use crate::repository_files::{FileKind, RepositoryFile, unknown_kind_report};
use crate::rules::{file_findings, missing_forbid};
use std::fmt;
use std::path::{Path, PathBuf};

pub struct CrateRule {
    pub root: &'static str,
    pub findings: fn(&Crate<'_>) -> Result<Vec<Finding>, Error>,
}

const CRATE_RULES: [CrateRule; 1] = [RENDERER_TEST_RULES];

#[derive(Clone, Copy, Debug)]
pub enum RuleSet {
    Rust,
    FileKind,
    PythonPlace,
    Comment,
    Document,
    Crate,
    Copy,
}

impl fmt::Display for RuleSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Rust => "Rust source rules",
            Self::FileKind => "file kind rules",
            Self::PythonPlace => "Python place rules",
            Self::Comment => "comment rules",
            Self::Document => "document rules",
            Self::Crate => "crate rules",
            Self::Copy => "copy rules",
        })
    }
}

pub fn repository_reports(
    files: &[RepositoryFile],
    root: &Path,
    crate_roots: &[PathBuf],
) -> Result<Vec<(RuleSet, Report)>, Error> {
    Ok(vec![
        (RuleSet::Rust, rust_report(files, crate_roots)?),
        (RuleSet::FileKind, unknown_kind_report(files)),
        (
            RuleSet::PythonPlace,
            python_place_report(files, &root.join(PYTHON_DIRECTORY)),
        ),
        (RuleSet::Comment, comment_report(files)?),
        (
            RuleSet::Document,
            document_report(files, &root.join(ALLOWED_DOCUMENT)),
        ),
    ])
}

pub fn workspace_reports(
    files: &[RepositoryFile],
    root: &Path,
) -> Result<Vec<(RuleSet, Report)>, Error> {
    Ok(vec![
        (RuleSet::Crate, crate_report(root)?),
        (RuleSet::Copy, copy_report(files, root)?),
    ])
}

pub fn rust_report(files: &[RepositoryFile], crate_roots: &[PathBuf]) -> Result<Report, Error> {
    let sources = parse_rust_files(files)?;
    let mut findings = Vec::new();
    for source in sources.values() {
        findings.extend(file_findings(source).map_err(|error| Error::ParseSource {
            path: source.path.clone(),
            source: error,
        })?);
    }
    findings.extend(hand_written_value_lists(&sources)?);
    for root in crate_roots {
        let crate_files = Crate::read(root, &sources)?;
        findings.extend(
            missing_forbid(&crate_files).map_err(|source| Error::ParseSource {
                path: root.clone(),
                source,
            })?,
        );
        for rule in CRATE_RULES.iter().filter(|rule| root.ends_with(rule.root)) {
            findings.extend((rule.findings)(&crate_files)?);
        }
    }
    findings.sort_by(|first, second| (&first.path, first.line).cmp(&(&second.path, second.line)));
    Ok(Report {
        findings,
        checked: sources.len(),
    })
}

fn parse_rust_files(files: &[RepositoryFile]) -> Result<Sources, Error> {
    files
        .iter()
        .filter_map(|file| match &file.kind {
            Some(FileKind::Source { language, text }) if language.is(&RUST) => {
                Some(parse_source(&file.path, text))
            }
            _ => None,
        })
        .map(|source| source.map(|source| (source.path.clone(), source)))
        .collect()
}

fn parse_source(path: &Path, text: &str) -> Result<Source, Error> {
    let syntax = syn::parse_file(text).map_err(|source| Error::ParseSource {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Source {
        path: path.to_path_buf(),
        syntax,
    })
}
