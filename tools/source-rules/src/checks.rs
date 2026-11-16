// SPDX-License-Identifier: GPL-3.0-only
use crate::comments::comment_report;
use crate::documents::{ALLOWED_DOCUMENT, document_report};
use crate::error::Error;
use crate::findings::Report;
use crate::languages::RUST;
use crate::modules::{Crate, Source, Sources};
use crate::repository_files::{FileKind, RepositoryFile, unknown_kind_report};
use crate::rules::{file_findings, missing_forbid};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum RuleSet {
    Rust,
    FileKind,
    Comment,
    Document,
}

impl fmt::Display for RuleSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Rust => "Rust source rules",
            Self::FileKind => "file kind rules",
            Self::Comment => "comment rules",
            Self::Document => "document rules",
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
        (RuleSet::Comment, comment_report(files)?),
        (
            RuleSet::Document,
            document_report(files, &root.join(ALLOWED_DOCUMENT)),
        ),
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
    for root in crate_roots {
        let crate_files = Crate::read(root, &sources)?;
        findings.extend(
            missing_forbid(&crate_files).map_err(|source| Error::ParseSource {
                path: root.clone(),
                source,
            })?,
        );
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
