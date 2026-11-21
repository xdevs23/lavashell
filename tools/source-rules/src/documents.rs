// SPDX-License-Identifier: GPL-3.0-only
use crate::findings::{Finding, Report, Rule};
use crate::repository_files::{FileKind, RepositoryFile};
use std::path::Path;

pub const ALLOWED_DOCUMENTS: &[&str] = &["AGENTS.md", "README.md"];

pub fn document_report(files: &[RepositoryFile], root: &Path) -> Report {
    let documents: Vec<_> = files
        .iter()
        .filter(|file| matches!(file.kind, Some(FileKind::Document)))
        .collect();
    Report {
        checked: documents.len(),
        findings: documents
            .into_iter()
            .filter(|document| {
                !(ALLOWED_DOCUMENTS.iter()).any(|allowed| document.path == root.join(allowed))
            })
            .map(|document| Finding {
                path: document.path.clone(),
                line: None,
                rule: Rule::Document {
                    allowed: ALLOWED_DOCUMENTS,
                },
            })
            .collect(),
    }
}
