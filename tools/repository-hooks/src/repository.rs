// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use std::collections::BTreeSet;
use std::fmt;
use std::io::{BufRead, BufReader, ErrorKind, PipeReader, Read, Write};
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// https://github.com/git/git/blob/v2.55.0/Documentation/git-commit.adoc#L253
const CUT_LINE: &str = "------------------------ >8 ------------------------\n";
// https://github.com/git/git/blob/v2.55.0/Documentation/diff-format.adoc
const RAW_CHANGES: [&str; 7] = [
    "--raw",
    "-z",
    "--no-abbrev",
    "--no-renames",
    "--no-relative",
    "--no-color",
    "--ignore-submodules=all",
];
const COMMITS_WITH_MESSAGE: [&str; 6] = [
    "--ignore-missing",
    "--no-show-signature",
    "--encoding=UTF-8",
    "--no-diff-merges",
    "--root",
    "--format=%H%x00%B",
];

#[derive(Debug, Clone, Copy)]
pub enum GitSubcommand {
    RevParse,
    Diff,
    Log,
    Stripspace,
    CatFile,
}

impl fmt::Display for GitSubcommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RevParse => "rev-parse",
            Self::Diff => "diff",
            Self::Log => "log",
            Self::Stripspace => "stripspace",
            Self::CatFile => "cat-file",
        })
    }
}

#[derive(Clone)]
pub struct CommitId(String);

impl CommitId {
    pub fn existing(object_name: &str) -> Option<Self> {
        names_an_object(object_name).then(|| Self(object_name.to_owned()))
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:.12}", self.0)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct BlobId(String);

pub struct ChangedFile {
    pub path: String,
    pub blob: BlobId,
}

pub struct PushedCommit {
    pub id: CommitId,
    pub message: String,
    pub files: Vec<ChangedFile>,
}

pub struct RefUpdate {
    pub pushed: CommitId,
    pub replaced: Option<CommitId>,
}

#[derive(Clone, Copy)]
pub enum Cleanup {
    Strip,
    Whitespace,
}

pub struct Repository {
    directory: PathBuf,
}

impl Repository {
    pub fn in_directory(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn common_directory(&self) -> Result<PathBuf, Error> {
        let path = self.read(
            GitSubcommand::RevParse,
            ["--path-format=absolute", "--git-common-dir"],
            "",
        )?;
        Ok(PathBuf::from(path.trim_end_matches('\n')))
    }

    pub fn staged_files(&self) -> Result<Vec<ChangedFile>, Error> {
        let changes = self.read(
            GitSubcommand::Diff,
            ["--cached"].into_iter().chain(RAW_CHANGES),
            "",
        )?;
        read_changed_files(
            GitSubcommand::Diff,
            &mut changes.split_terminator('\0').peekable(),
        )
    }

    pub fn pushed_commits(
        &self,
        remote: &str,
        updates: &[RefUpdate],
    ) -> Result<Vec<PushedCommit>, Error> {
        let remote_refs = format!("--remotes={remote}");
        let revisions: String = updates
            .iter()
            .flat_map(|update| {
                let replaced = update.replaced.as_ref();
                [
                    Some(format!("{}\n", update.pushed.0)),
                    replaced.map(|commit| format!("^{}\n", commit.0)),
                ]
            })
            .flatten()
            .collect();
        let arguments = COMMITS_WITH_MESSAGE.into_iter().chain(RAW_CHANGES).chain([
            "--stdin",
            "--not",
            &remote_refs,
        ]);
        let log = self.read(GitSubcommand::Log, arguments, &revisions)?;
        let mut fields = log.split_terminator('\0').peekable();
        let mut commits = Vec::new();
        while let Some(id) = fields.next() {
            let message = fields.next().ok_or_else(|| Error::GitOutput {
                subcommand: GitSubcommand::Log,
                output: id.to_owned(),
            })?;
            commits.push(PushedCommit {
                id: CommitId(id.to_owned()),
                message: message.to_owned(),
                files: read_changed_files(GitSubcommand::Log, &mut fields)?,
            });
        }
        Ok(commits)
    }

    pub fn message_in_file(&self, path: &Path, cleanup: Cleanup) -> Result<String, Error> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::ReadMessage {
            path: path.to_owned(),
            source,
        })?;
        match cleanup {
            Cleanup::Whitespace => self.read(GitSubcommand::Stripspace, [], &text),
            Cleanup::Strip => {
                let cut_line =
                    self.read(GitSubcommand::Stripspace, ["--comment-lines"], CUT_LINE)?;
                let above_cut_line: String = text
                    .split_inclusive('\n')
                    .take_while(|line| *line != cut_line)
                    .collect();
                self.read(
                    GitSubcommand::Stripspace,
                    ["--strip-comments"],
                    &above_cut_line,
                )
            }
        }
    }

    pub fn read_blobs<'blob>(
        &self,
        blobs: &BTreeSet<&'blob BlobId>,
        mut inspect: impl FnMut(&'blob BlobId, &[u8]),
    ) -> Result<(), Error> {
        let requests: String = blobs
            .iter()
            .flat_map(|blob| [blob.0.as_str(), "\n"])
            .collect();
        self.run(GitSubcommand::CatFile, ["--batch"], &requests, |output| {
            let mut output = BufReader::new(output);
            blobs.iter().try_for_each(|blob| {
                inspect(blob, &read_blob(&mut output, blob)?);
                Ok(())
            })
        })
    }

    fn read<'a>(
        &self,
        subcommand: GitSubcommand,
        arguments: impl IntoIterator<Item = &'a str>,
        input: &str,
    ) -> Result<String, Error> {
        self.run(subcommand, arguments, input, |mut output| {
            let mut text = Vec::new();
            let _length_already_in_text = output
                .read_to_end(&mut text)
                .map_err(|source| Error::ReadGitOutput { subcommand, source })?;
            String::from_utf8(text).map_err(|source| Error::DecodeGitOutput { subcommand, source })
        })
    }

    fn run<'a, Output>(
        &self,
        subcommand: GitSubcommand,
        arguments: impl IntoIterator<Item = &'a str>,
        input: &str,
        read_output: impl FnOnce(PipeReader) -> Result<Output, Error>,
    ) -> Result<Output, Error> {
        let (input_read_by_git, mut input_writer) = std::io::pipe().map_err(Error::StartGit)?;
        let (output, output_written_by_git) = std::io::pipe().map_err(Error::StartGit)?;
        let mut git = Command::new("git")
            .current_dir(&self.directory)
            .arg(subcommand.to_string())
            .args(arguments)
            .stdin(input_read_by_git)
            .stdout(output_written_by_git)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| match source.kind() {
                ErrorKind::NotFound => Error::FindGit(source),
                _ => Error::StartGit(source),
            })?;
        let (sent, read) = std::thread::scope(|scope| {
            let sender = scope.spawn(move || input_writer.write_all(input.as_bytes()));
            let read = read_output(output);
            let sent = sender
                .join()
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
            (sent, read)
        });
        let status = git
            .wait()
            .map_err(|source| Error::WaitForGit { subcommand, source })?;
        if !status.success() {
            return Err(Error::Git { subcommand, status });
        }
        sent.map_err(|source| Error::SendGitInput { subcommand, source })?;
        read
    }
}

// https://github.com/git/git/blob/v2.55.0/Documentation/githooks.adoc#L250
fn names_an_object(object_name: &str) -> bool {
    object_name.bytes().any(|digit| digit != b'0')
}

fn read_changed_files<'output>(
    subcommand: GitSubcommand,
    fields: &mut Peekable<impl Iterator<Item = &'output str>>,
) -> Result<Vec<ChangedFile>, Error> {
    let mut files = Vec::new();
    while let Some(change) = fields.next_if(|field| field.trim_start().starts_with(':')) {
        let (Some(path), [_old_mode, _mode, _old_blob, blob, _status]) =
            (fields.next(), &change.split(' ').collect::<Vec<_>>()[..])
        else {
            return Err(Error::GitOutput {
                subcommand,
                output: change.to_owned(),
            });
        };
        files.extend(names_an_object(blob).then(|| ChangedFile {
            path: path.to_owned(),
            blob: BlobId((*blob).to_owned()),
        }));
    }
    Ok(files)
}

// https://github.com/git/git/blob/v2.55.0/Documentation/git-cat-file.adoc#L291
fn read_blob(output: &mut impl BufRead, blob: &BlobId) -> Result<Vec<u8>, Error> {
    let subcommand = GitSubcommand::CatFile;
    let read_failed = |source| Error::ReadGitOutput { subcommand, source };
    let mut header = String::new();
    let _length_already_in_header = output.read_line(&mut header).map_err(read_failed)?;
    let size = header
        .strip_prefix(&format!("{} blob ", blob.0))
        .and_then(|size| size.trim_end().parse().ok())
        .ok_or(Error::GitOutput {
            subcommand,
            output: header,
        })?;
    let mut content = Vec::new();
    let _length_already_in_content = output
        .by_ref()
        .take(size)
        .read_to_end(&mut content)
        .map_err(read_failed)?;
    let mut line_end = [0];
    output.read_exact(&mut line_end).map_err(read_failed)?;
    Ok(content)
}
