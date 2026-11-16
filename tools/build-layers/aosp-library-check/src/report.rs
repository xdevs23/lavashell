// SPDX-License-Identifier: GPL-3.0-only
use crate::elf_library::ElfError;
use crate::system_stubs::StubError;
use crate::verdict::Verdict;
use serde::{Serialize, Serializer};
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum InspectionError {
    #[error("the library {path} cannot be read: {source}")]
    ReadLibrary {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the library {path} is not a valid ELF library: {source}")]
    ParseLibrary {
        path: PathBuf,
        #[source]
        source: ElfError,
    },
    #[error(transparent)]
    Stubs(#[from] StubError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Pass,
    Fail,
    Error,
}

impl Status {
    #[must_use]
    pub fn exit_code(self) -> ExitCode {
        ExitCode::from(match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Error => 2,
        })
    }

    fn next_step(self) -> Option<NextStep> {
        match self {
            Self::Pass => None,
            Self::Fail => Some(NextStep::FixListedProblems),
            Self::Error => Some(NextStep::FixCheckError),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NextStep {
    FixListedProblems,
    FixCheckError,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FixListedProblems => "Fix each listed problem, then run the check again.",
            Self::FixCheckError => {
                "The check could not run. Fix the cause named in the error, then run the check again."
            }
        })
    }
}

pub struct LibraryDigest(pub [u8; 32]);

impl fmt::Display for LibraryDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .iter()
            .try_for_each(|byte| write!(formatter, "{byte:02x}"))
    }
}

pub struct Report {
    pub library: PathBuf,
    pub system_libraries: PathBuf,
    pub sha256: Option<LibraryDigest>,
    pub outcome: Result<Verdict, InspectionError>,
}

impl Report {
    #[must_use]
    pub fn status(&self) -> Status {
        match &self.outcome {
            Ok(verdict) if verdict.problems.is_empty() => Status::Pass,
            Ok(_failed) => Status::Fail,
            Err(_error) => Status::Error,
        }
    }
}

#[derive(Serialize)]
struct ReportDocument<'report> {
    library: &'report PathBuf,
    system_libraries: &'report PathBuf,
    sha256: Option<Text<&'report LibraryDigest>>,
    status: Status,
    verdict: Option<&'report Verdict>,
    error: Option<Text<&'report InspectionError>>,
    next_step: Option<Text<NextStep>>,
}

impl Serialize for Report {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let status = self.status();
        ReportDocument {
            library: &self.library,
            system_libraries: &self.system_libraries,
            sha256: self.sha256.as_ref().map(Text),
            status,
            verdict: self.outcome.as_ref().ok(),
            error: self.outcome.as_ref().err().map(Text),
            next_step: status.next_step().map(Text),
        }
        .serialize(serializer)
    }
}

struct Text<Value>(Value);

impl<Value: fmt::Display> Serialize for Text<Value> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}
