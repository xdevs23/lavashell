// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use std::fmt;

pub enum NextStep {
    EnterDevelopmentShell,
    FixCargoWorkspace,
    FixNamedFile,
    AddLanguage,
    ReportDefect,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EnterDevelopmentShell => {
                "Run the source rules inside `nix develop`, which provides cargo."
            }
            Self::FixCargoWorkspace => concat!(
                "Make `cargo metadata --no-deps --offline` succeed in the repository, ",
                "then run the source rules again.",
            ),
            Self::FixNamedFile => {
                "Correct or remove the file named above, then run the source rules again."
            }
            Self::AddLanguage => concat!(
                "Add the language of the patched file to the languages of the source rules, ",
                "or remove that file from the patch, then run the source rules again.",
            ),
            Self::ReportDefect => {
                "The source rules are defective. Report this output to the developers of the project."
            }
        })
    }
}

pub fn next_step(error: &Error) -> NextStep {
    match error {
        Error::StartMetadata(_) => NextStep::EnterDevelopmentShell,
        Error::Metadata(_) => NextStep::FixCargoWorkspace,
        Error::ReadSource { .. }
        | Error::ParseSource { .. }
        | Error::ParsePatch { .. }
        | Error::ReadManifest { .. }
        | Error::ReadCopy { .. }
        | Error::ListFiles(_)
        | Error::ModuleFile { .. } => NextStep::FixNamedFile,
        Error::PatchedLanguage { .. } => NextStep::AddLanguage,
        Error::ReadMetadata(_)
        | Error::ParseTree { .. }
        | Error::LoadGrammar { .. }
        | Error::CommentQuery { .. }
        | Error::CrateRoot { .. }
        | Error::CopyPosition { .. }
        | Error::NoParentDirectory { .. } => NextStep::ReportDefect,
    }
}
