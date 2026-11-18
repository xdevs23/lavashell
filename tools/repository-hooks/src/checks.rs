// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::findings::{CheckedCommit, Finding, Violation};
use crate::repository::{BlobId, ChangedFile, Cleanup, RefUpdate, Repository};
use crate::word_rules::{ListedLine, WordRules};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type ListedLines<'file> = BTreeMap<&'file BlobId, Vec<ListedLine>>;

pub struct HookChecks {
    repository: Repository,
    rules: WordRules,
}

impl HookChecks {
    pub fn in_directory(directory: impl Into<PathBuf>) -> Result<Self, Error> {
        let repository = Repository::in_directory(directory);
        let rules = WordRules::read(&repository)?;
        Ok(Self { repository, rules })
    }

    pub fn staged_file_findings(&self) -> Result<Vec<Finding>, Error> {
        let files = self.repository.staged_files()?;
        let listed_lines = self.listed_lines_by_blob(&files)?;
        Ok(found_in(
            &CheckedCommit::BeingCreated,
            self.changed_file_violations(&files, &listed_lines),
        ))
    }

    pub fn message_file_findings(
        &self,
        path: &Path,
        cleanup: Cleanup,
    ) -> Result<Vec<Finding>, Error> {
        let message = self.repository.message_in_file(path, cleanup)?;
        Ok(found_in(
            &CheckedCommit::BeingCreated,
            self.message_violations(&message),
        ))
    }

    pub fn pushed_commit_findings(
        &self,
        remote: &str,
        updates: &[RefUpdate],
    ) -> Result<Vec<Finding>, Error> {
        let commits = self.repository.pushed_commits(remote, updates)?;
        let listed_lines =
            self.listed_lines_by_blob(commits.iter().flat_map(|commit| &commit.files))?;
        Ok(commits
            .iter()
            .flat_map(|commit| {
                let in_message = self.message_violations(&commit.message);
                let in_files = self.changed_file_violations(&commit.files, &listed_lines);
                found_in(
                    &CheckedCommit::Pushed(commit.id.clone()),
                    in_message.into_iter().chain(in_files),
                )
            })
            .collect())
    }

    fn listed_lines_by_blob<'file>(
        &self,
        files: impl IntoIterator<Item = &'file ChangedFile>,
    ) -> Result<ListedLines<'file>, Error> {
        let blobs = files.into_iter().map(|file| &file.blob).collect();
        let mut listed_lines = BTreeMap::new();
        self.repository.read_blobs(&blobs, |blob, content| {
            listed_lines.extend([(blob, self.rules.listed_lines(content))]);
        })?;
        Ok(listed_lines)
    }

    fn changed_file_violations(
        &self,
        files: &[ChangedFile],
        listed_lines: &ListedLines<'_>,
    ) -> Vec<Violation> {
        let in_content = files.iter().flat_map(|file| {
            let lines = listed_lines.get(&file.blob).into_iter().flatten();
            lines.map(|line| Violation::ListedWordInContent {
                path: file.path.clone(),
                line: line.number,
                text: line.text.clone(),
            })
        });
        let in_paths = files
            .iter()
            .filter(|file| self.rules.contains_listed_word(file.path.as_bytes()))
            .map(|file| Violation::ListedWordInPath {
                path: file.path.clone(),
            });
        in_content.chain(in_paths).collect()
    }

    fn message_violations(&self, message: &str) -> Vec<Violation> {
        let lines: Vec<&str> = message.trim_end_matches('\n').lines().collect();
        let one_title = matches!(lines[..], [title] if !title.trim().is_empty());
        let not_one_title = (!one_title).then_some(Violation::MessageNotOneTitle);
        let listed_word = lines
            .iter()
            .find(|line| self.rules.contains_listed_word(line.as_bytes()))
            .map(|line| Violation::ListedWordInMessage {
                line: (*line).to_owned(),
            });
        not_one_title.into_iter().chain(listed_word).collect()
    }
}

fn found_in(
    commit: &CheckedCommit,
    violations: impl IntoIterator<Item = Violation>,
) -> Vec<Finding> {
    violations
        .into_iter()
        .map(|violation| Finding {
            commit: commit.clone(),
            violation,
        })
        .collect()
}
