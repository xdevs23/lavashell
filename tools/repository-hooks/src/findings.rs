// SPDX-License-Identifier: GPL-3.0-only
use crate::repository::CommitId;
use std::fmt;

#[derive(Clone)]
pub enum CheckedCommit {
    BeingCreated,
    Pushed(CommitId),
}

pub enum Violation {
    ListedWordInContent {
        path: String,
        line: usize,
        text: String,
    },
    ListedWordInPath {
        path: String,
    },
    ListedWordInMessage {
        line: String,
    },
    MessageNotOneTitle,
}

pub struct Finding {
    pub commit: CheckedCommit,
    pub violation: Violation,
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let CheckedCommit::Pushed(commit) = &self.commit {
            write!(formatter, "{commit}: ")?;
        }
        match &self.violation {
            Violation::ListedWordInContent { path, line, text } => write!(
                formatter,
                "{path}:{line}: the line contains a word from the local word list: {text}",
            ),
            Violation::ListedWordInPath { path } => write!(
                formatter,
                "{path}: the file name contains a word from the local word list",
            ),
            Violation::ListedWordInMessage { line } => write!(
                formatter,
                "the commit message contains a word from the local word list: {line}",
            ),
            Violation::MessageNotOneTitle => formatter.write_str(
                "the commit message must be exactly one title line, without a body or trailers",
            ),
        }
    }
}
