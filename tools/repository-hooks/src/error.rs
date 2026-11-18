// SPDX-License-Identifier: GPL-3.0-only
use crate::repository::GitSubcommand;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::string::FromUtf8Error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the hook cannot find git")]
    FindGit(#[source] std::io::Error),
    #[error("the hook cannot start git")]
    StartGit(#[source] std::io::Error),
    #[error("the hook cannot send input to git {subcommand}")]
    SendGitInput {
        subcommand: GitSubcommand,
        #[source]
        source: std::io::Error,
    },
    #[error("the hook cannot read the output from git {subcommand}")]
    ReadGitOutput {
        subcommand: GitSubcommand,
        #[source]
        source: std::io::Error,
    },
    #[error("the hook cannot wait for git {subcommand} to exit")]
    WaitForGit {
        subcommand: GitSubcommand,
        #[source]
        source: std::io::Error,
    },
    #[error("git {subcommand} exited with {status}")]
    Git {
        subcommand: GitSubcommand,
        status: ExitStatus,
    },
    #[error("git {subcommand} returns output that is not UTF-8")]
    DecodeGitOutput {
        subcommand: GitSubcommand,
        #[source]
        source: FromUtf8Error,
    },
    #[error("git {subcommand} printed {output:?} in an unexpected form")]
    GitOutput {
        subcommand: GitSubcommand,
        output: String,
    },
    #[error("the hook cannot read the word list {path}")]
    ReadWordList {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the word list {path} is missing or empty, so the hook cannot check for its words")]
    NoListedWords { path: PathBuf },
    #[error("the word list {path} is not a list of regular expressions")]
    CompileWordList {
        path: PathBuf,
        #[source]
        source: regex::Error,
    },
    #[error("the hook cannot read the commit message {path}")]
    ReadMessage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the hook cannot read the pushed refs from the standard input")]
    ReadPushedRefs(#[source] std::io::Error),
    #[error("the line {line:?} of the pushed refs does not have four fields")]
    PushedRef { line: String },
}
