// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::findings::{Finding, Report, Rule};
use crate::repository_files::RepositoryFile;
use std::collections::BTreeSet;
use std::path::{Component, Path};

const EXAMPLES: &str = "examples";
const CATALOG_DIRECTORY: &str = "copy";
const CATALOG_SUFFIX: &str = ".en.tsv";
const CATALOG_COLUMNS: usize = 2;
const UNSUPPORTED_PUNCTUATION: [char; 3] = [';', '—', '–'];

fn is_catalog(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components: Vec<Component<'_>> = relative.components().collect();
    matches!(
        components.as_slice(),
        [Component::Normal(examples), Component::Normal(_), Component::Normal(directory), Component::Normal(name)]
            if *examples == EXAMPLES
                && *directory == CATALOG_DIRECTORY
                && name.to_str().is_some_and(|name| name.ends_with(CATALOG_SUFFIX))
    )
}

fn catalog_rules(path: &Path) -> Result<Vec<(usize, Rule)>, Error> {
    let read_error = |source| Error::ReadCopy {
        path: path.to_owned(),
        source,
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .quoting(false)
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(read_error)?;
    let mut keys = BTreeSet::new();
    let mut rules = Vec::new();
    for row in reader.records() {
        let row = row.map_err(read_error)?;
        let line = row
            .position()
            .map(csv::Position::line)
            .ok_or_else(|| Error::CopyPosition {
                path: path.to_owned(),
            })
            .and_then(|line| {
                usize::try_from(line).map_err(|_too_large| Error::CopyPosition {
                    path: path.to_owned(),
                })
            })?;
        let (Some(key), Some(value), CATALOG_COLUMNS) = (row.get(0), row.get(1), row.len()) else {
            rules.push((line, Rule::CopyColumns));
            continue;
        };
        if !keys.insert(key.to_owned()) {
            rules.push((
                line,
                Rule::DuplicateCopyKey {
                    key: key.to_owned(),
                },
            ));
        }
        if value.trim().is_empty() {
            rules.push((
                line,
                Rule::EmptyCopy {
                    key: key.to_owned(),
                },
            ));
        }
        if value.contains(UNSUPPORTED_PUNCTUATION) {
            rules.push((
                line,
                Rule::CopyPunctuation {
                    key: key.to_owned(),
                },
            ));
        }
    }
    Ok(rules)
}

pub fn copy_report(files: &[RepositoryFile], root: &Path) -> Result<Report, Error> {
    let catalogs: Vec<&Path> = files
        .iter()
        .map(|file| file.path.as_path())
        .filter(|path| is_catalog(path, root))
        .collect();
    if catalogs.is_empty() {
        return Ok(Report {
            findings: vec![Finding {
                path: root.join(EXAMPLES),
                line: None,
                rule: Rule::NoCopyCatalogs,
            }],
            checked: 0,
        });
    }
    let mut findings = Vec::new();
    for catalog in &catalogs {
        findings.extend(
            catalog_rules(catalog)?
                .into_iter()
                .map(|(line, rule)| Finding {
                    path: catalog.to_path_buf(),
                    line: Some(line),
                    rule,
                }),
        );
    }
    Ok(Report {
        findings,
        checked: catalogs.len(),
    })
}
