// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::findings::{Finding, Report, Rule};
use crate::grammar::{Grammar, Grammars, source_of};
use crate::languages::{CommentMarkers, HASH, Language};
use crate::patch::patched_files;
use crate::repository_files::{FileKind, RepositoryFile};
use std::collections::BTreeSet;
use std::path::Path;
use tree_sitter::{Node, Point};

const LINK_SCHEMES: [&str; 2] = ["http", "https"];

struct Allowed<'text> {
    language: &'static Language,
    text: &'text str,
    necessary: BTreeSet<usize>,
    license_header: Option<Point>,
}

impl Allowed<'_> {
    fn permits(&self, comment: &Node<'_>) -> bool {
        let source = source_of(comment, self.text);
        self.necessary.contains(&comment.id())
            || self.is_preamble(comment, source)
            || self.is_license_header(comment, source)
            || is_link(source, self.language.markers)
    }

    fn is_preamble(&self, comment: &Node<'_>, source: &str) -> bool {
        comment.start_byte() == 0 && self.language.has_preamble(source)
    }

    fn is_license_header(&self, comment: &Node<'_>, source: &str) -> bool {
        Some(comment.start_position()) == self.license_header
            && declares_spdx_license(source, self.language.markers)
    }
}

fn comment_text(comment: &str, markers: CommentMarkers) -> Option<&str> {
    comment
        .strip_prefix(markers.open)?
        .strip_suffix(markers.close)?
        .strip_prefix(' ')
}

fn declares_spdx_license(comment: &str, markers: CommentMarkers) -> bool {
    comment_text(comment, markers)
        .and_then(|text| text.strip_prefix("SPDX-License-Identifier: "))
        .is_some_and(|expression| spdx::Expression::parse(expression).is_ok())
}

fn is_link(comment: &str, markers: CommentMarkers) -> bool {
    comment_text(comment, markers).is_some_and(|text| {
        !text.contains(char::is_whitespace)
            && url::Url::parse(text).is_ok_and(|url| LINK_SCHEMES.contains(&url.scheme()))
    })
}

fn refused_rows(
    grammar: &mut Grammar,
    path: &Path,
    text: &str,
    license_header: Option<Point>,
) -> Result<Vec<usize>, Error> {
    let tree = grammar.parse(path, text)?;
    let comments = grammar.comment_nodes(&tree, text);
    let language = grammar.language;
    let allowed = Allowed {
        language,
        text,
        necessary: (language.necessary_comments)(&comments, text),
        license_header,
    };
    Ok(comments
        .iter()
        .filter(|comment| !allowed.permits(comment))
        .map(|comment| comment.start_position().row)
        .collect())
}

fn source_comment_lines(
    grammar: &mut Grammar,
    path: &Path,
    text: &str,
) -> Result<Vec<usize>, Error> {
    let header_row = usize::from(grammar.language.has_preamble(text));
    let rows = refused_rows(grammar, path, text, Some(Point::new(header_row, 0)))?;
    Ok(rows.into_iter().map(|row| row + 1).collect())
}

fn pattern_list_comment_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with(HASH.open))
        .filter(|(row, line)| *row != 0 || !declares_spdx_license(line, HASH))
        .map(|(row, _)| row + 1)
        .collect()
}

fn patch_comment_lines(
    grammars: &mut Grammars,
    path: &Path,
    diff: &str,
) -> Result<Vec<usize>, Error> {
    let mut lines = Vec::new();
    let patched_files = patched_files(diff).map_err(|source| Error::ParsePatch {
        path: path.to_path_buf(),
        source,
    })?;
    for patched in patched_files {
        let language = patched.language().ok_or_else(|| Error::PatchedLanguage {
            patch: path.to_path_buf(),
            patched: patched.path.clone(),
        })?;
        let rows = refused_rows(grammars.of(language)?, path, &patched.added_text(), None)?;
        lines.extend(
            rows.into_iter()
                .map(|row| patched.patch_line_of_added_row(row)),
        );
    }
    Ok(lines)
}

pub fn comment_report(files: &[RepositoryFile]) -> Result<Report, Error> {
    let mut grammars = Grammars::default();
    let mut report = Report::default();
    for file in files {
        let lines = match &file.kind {
            Some(FileKind::Source { language, text }) => {
                source_comment_lines(grammars.of(language)?, &file.path, text)?
            }
            Some(FileKind::Patch { text }) => patch_comment_lines(&mut grammars, &file.path, text)?,
            Some(FileKind::PatternList { text }) => pattern_list_comment_lines(text),
            Some(FileKind::Document | FileKind::Data) | None => continue,
        };
        report.checked += 1;
        report
            .findings
            .extend(lines.into_iter().map(|line| Finding {
                path: file.path.clone(),
                line: Some(line),
                rule: Rule::Comment,
            }));
    }
    Ok(report)
}
