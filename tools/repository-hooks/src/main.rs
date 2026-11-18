// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod checks;
mod error;
mod findings;
mod next_steps;
mod pushed_refs;
mod repository;
mod word_rules;

use checks::HookChecks;
use clap::Parser;
use error::Error;
use findings::Finding;
use lavashell_error_chain::message_chain;
use next_steps::next_step;
use pushed_refs::read_ref_updates;
use repository::Cleanup;
use std::path::PathBuf;
use std::process::ExitCode;

// https://github.com/git/git/blob/v2.55.0/Documentation/githooks.adoc#L106
const EDITOR_VARIABLE: &str = "GIT_EDITOR";
const NO_EDITOR: &str = ":";

#[derive(Parser)]
enum Hook {
    PreCommit,
    CommitMsg { message_file: PathBuf },
    PrePush { remote: String, location: String },
}

enum Verdict {
    Clean,
    Findings,
    Undecided,
}

impl From<Verdict> for ExitCode {
    fn from(verdict: Verdict) -> Self {
        Self::from(match verdict {
            Verdict::Clean => 0,
            Verdict::Findings => 1,
            Verdict::Undecided => 2,
        })
    }
}

fn main() -> ExitCode {
    let verdict = match findings_of(Hook::parse()) {
        Ok(findings) if findings.is_empty() => Verdict::Clean,
        Ok(findings) => {
            for finding in findings {
                eprintln!("{finding}");
            }
            Verdict::Findings
        }
        Err(error) => {
            eprintln!("{}\n{}", message_chain(&error), next_step(&error));
            Verdict::Undecided
        }
    };
    verdict.into()
}

fn findings_of(hook: Hook) -> Result<Vec<Finding>, Error> {
    let checks = HookChecks::in_directory(".")?;
    match hook {
        Hook::PreCommit => checks.staged_file_findings(),
        Hook::CommitMsg { message_file } => {
            checks.message_file_findings(&message_file, cleanup_applied_by_git())
        }
        Hook::PrePush {
            remote,
            location: _commits_on_the_remote_are_found_without_its_location,
        } => {
            let pushed_refs =
                std::io::read_to_string(std::io::stdin()).map_err(Error::ReadPushedRefs)?;
            checks.pushed_commit_findings(&remote, &read_ref_updates(&pushed_refs)?)
        }
    }
}

fn cleanup_applied_by_git() -> Cleanup {
    if std::env::var_os(EDITOR_VARIABLE).is_some_and(|editor| editor == NO_EDITOR) {
        Cleanup::Whitespace
    } else {
        Cleanup::Strip
    }
}
