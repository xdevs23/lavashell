// SPDX-License-Identifier: GPL-3.0-only
use crate::languages::Language;
use crate::repository_files::language_of_path;
use std::path::Path;
use unidiff::PatchSet;

pub struct PatchedFile {
    pub path: String,
    added: Vec<(usize, String)>,
}

impl PatchedFile {
    pub fn language(&self) -> Option<&'static Language> {
        language_of_path(Path::new(&self.path))
    }

    pub fn added_text(&self) -> String {
        let lines: Vec<_> = self.added.iter().map(|(_, text)| text.as_str()).collect();
        lines.join("\n")
    }

    pub fn patch_line_of_added_row(&self, row: usize) -> usize {
        self.added[row].0
    }
}

pub fn patched_files(diff: &str) -> Result<Vec<PatchedFile>, unidiff::Error> {
    let mut patch = PatchSet::new();
    patch.parse(diff)?;
    Ok(patch
        .into_iter()
        .map(|file| PatchedFile {
            path: file.path(),
            added: file
                .into_iter()
                .flatten()
                .filter(unidiff::Line::is_added)
                .map(|line| (line.diff_line_no, line.value))
                .collect(),
        })
        .collect())
}
