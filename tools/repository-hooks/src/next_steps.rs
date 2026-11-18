// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use std::fmt;

pub enum NextStep {
    ProvideGit,
    CorrectReportedCause,
    MakeWordListReadable,
    WriteWordList,
    ConvertToUtf8,
    ReportDefect,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProvideGit => "Put git on the PATH, then repeat the command.",
            Self::CorrectReportedCause => {
                "Correct the cause reported above, then repeat the git command."
            }
            Self::MakeWordListReadable => concat!(
                "Make the word list reported above a readable UTF-8 text file, ",
                "then repeat the git command.",
            ),
            Self::WriteWordList => concat!(
                "Write one regular expression per line into the word list reported above, ",
                "then repeat the git command.",
            ),
            Self::ConvertToUtf8 => concat!(
                "Convert the file names and the commit message to UTF-8, ",
                "then repeat the git command.",
            ),
            Self::ReportDefect => concat!(
                "The repository hooks are defective. ",
                "Report this output to the people who develop the project.",
            ),
        })
    }
}

pub fn next_step(error: &Error) -> NextStep {
    match error {
        Error::FindGit(_) => NextStep::ProvideGit,
        Error::StartGit(_)
        | Error::SendGitInput { .. }
        | Error::ReadGitOutput { .. }
        | Error::WaitForGit { .. }
        | Error::Git { .. }
        | Error::ReadMessage { .. }
        | Error::ReadPushedRefs(_) => NextStep::CorrectReportedCause,
        Error::ReadWordList { .. } => NextStep::MakeWordListReadable,
        Error::NoListedWords { .. } | Error::CompileWordList { .. } => NextStep::WriteWordList,
        Error::DecodeGitOutput { .. } => NextStep::ConvertToUtf8,
        Error::GitOutput { .. } | Error::PushedRef { .. } => NextStep::ReportDefect,
    }
}
