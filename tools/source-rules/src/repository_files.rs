// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::findings::{Finding, Report, Rule};
use crate::languages::{BASH, CPP, KOTLIN, Language, NIX, PYTHON, RUST, SKSL, TOML, XML};
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum FileMatch {
    Name(&'static str),
    Extension(&'static str),
}

#[derive(Clone, Copy)]
enum KnownKind {
    Source(&'static Language),
    Patch,
    PatternList,
    Document,
    Data,
}

static FILE_KINDS: &[(FileMatch, KnownKind)] = &[
    (FileMatch::Name("COPYING"), KnownKind::Data),
    (FileMatch::Name(".gitignore"), KnownKind::PatternList),
    (FileMatch::Name(".gitattributes"), KnownKind::PatternList),
    (FileMatch::Extension("md"), KnownKind::Document),
    (FileMatch::Extension("markdown"), KnownKind::Document),
    (FileMatch::Extension("mdx"), KnownKind::Document),
    (FileMatch::Extension("rst"), KnownKind::Document),
    (FileMatch::Extension("adoc"), KnownKind::Document),
    (FileMatch::Extension("txt"), KnownKind::Document),
    (FileMatch::Extension("json"), KnownKind::Data),
    (FileMatch::Extension("tsv"), KnownKind::Data),
    (FileMatch::Extension("lock"), KnownKind::Data),
    (FileMatch::Extension("png"), KnownKind::Data),
    (FileMatch::Extension("pdf"), KnownKind::Data),
    (FileMatch::Extension("patch"), KnownKind::Patch),
    (FileMatch::Extension("sh"), KnownKind::Source(&BASH)),
    (FileMatch::Extension("cpp"), KnownKind::Source(&CPP)),
    (FileMatch::Extension("h"), KnownKind::Source(&CPP)),
    (FileMatch::Extension("kt"), KnownKind::Source(&KOTLIN)),
    (FileMatch::Extension("kts"), KnownKind::Source(&KOTLIN)),
    (FileMatch::Extension("nix"), KnownKind::Source(&NIX)),
    (FileMatch::Extension("py"), KnownKind::Source(&PYTHON)),
    (FileMatch::Extension("pyi"), KnownKind::Source(&PYTHON)),
    (FileMatch::Extension("rs"), KnownKind::Source(&RUST)),
    (FileMatch::Extension("sksl"), KnownKind::Source(&SKSL)),
    (FileMatch::Extension("toml"), KnownKind::Source(&TOML)),
    (FileMatch::Extension("xml"), KnownKind::Source(&XML)),
];

impl FileMatch {
    fn matches(self, path: &Path) -> bool {
        let (part, expected) = match self {
            Self::Name(name) => (path.file_name(), name),
            Self::Extension(extension) => (path.extension(), extension),
        };
        part.and_then(OsStr::to_str)
            .is_some_and(|part| part.eq_ignore_ascii_case(expected))
    }
}

fn known_kind(path: &Path) -> Option<KnownKind> {
    FILE_KINDS
        .iter()
        .find(|(file_match, _)| file_match.matches(path))
        .map(|(_, kind)| *kind)
}

pub fn language_of_path(path: &Path) -> Option<&'static Language> {
    match known_kind(path)? {
        KnownKind::Source(language) => Some(language),
        KnownKind::Patch | KnownKind::PatternList | KnownKind::Document | KnownKind::Data => None,
    }
}

pub enum FileKind {
    Source {
        language: &'static Language,
        text: String,
    },
    Patch {
        text: String,
    },
    PatternList {
        text: String,
    },
    Document,
    Data,
}

pub struct RepositoryFile {
    pub path: PathBuf,
    pub kind: Option<FileKind>,
}

fn read_text(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|source| Error::ReadSource {
        path: path.to_path_buf(),
        source,
    })
}

fn kind_of(path: &Path) -> Result<Option<FileKind>, Error> {
    let Some(kind) = known_kind(path) else {
        return Ok(None);
    };
    Ok(Some(match kind {
        KnownKind::Source(language) => FileKind::Source {
            language,
            text: read_text(path)?,
        },
        KnownKind::Patch => FileKind::Patch {
            text: read_text(path)?,
        },
        KnownKind::PatternList => FileKind::PatternList {
            text: read_text(path)?,
        },
        KnownKind::Document => FileKind::Document,
        KnownKind::Data => FileKind::Data,
    }))
}

pub fn read_repository_files(root: &Path) -> Result<Vec<RepositoryFile>, Error> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root)
        .standard_filters(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .sort_by_file_name(Ord::cmp)
        .build()
    {
        let entry = entry.map_err(Error::ListFiles)?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            files.push(RepositoryFile {
                kind: kind_of(entry.path())?,
                path: entry.into_path(),
            });
        }
    }
    Ok(files)
}

pub fn unknown_kind_report(files: &[RepositoryFile]) -> Report {
    Report {
        checked: files.len(),
        findings: files
            .iter()
            .filter(|file| file.kind.is_none())
            .map(|file| Finding {
                path: file.path.clone(),
                line: None,
                rule: Rule::UnknownFileKind,
            })
            .collect(),
    }
}
