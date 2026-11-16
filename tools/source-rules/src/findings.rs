// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Rule {
    WildcardBinding,
    SilencedLint,
    BoolOutcome,
    MissingForbid,
    IgnoredTest,
    PanickingTest,
    ErasedError,
    Comment,
    Document { allowed: &'static str },
    UnknownFileKind,
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WildcardBinding => formatter.write_str(
                "bind the value to a name that says why it is not needed, never to `_`",
            ),
            Self::SilencedLint => formatter.write_str(
                "never allow, expect or lower a lint with an attribute, fix what it reports",
            ),
            Self::BoolOutcome => formatter.write_str(
                "report the outcome as an enum named for its meanings or as an error variant, never as `Ok(bool)`",
            ),
            Self::MissingForbid => formatter.write_str(
                "the crate has no unsafe code and must start with `#![forbid(unsafe_code)]`",
            ),
            Self::IgnoredTest => formatter.write_str(
                "run the test and fail it where it lacks a capability, never mark it `#[ignore]`",
            ),
            Self::PanickingTest => formatter.write_str(
                "match the error that the code returns, never mark the test `#[should_panic]`",
            ),
            Self::ErasedError => formatter.write_str(
                "report a failure as the error enum of the crate, never as a `String` or a boxed `dyn Error`",
            ),
            Self::Comment => formatter.write_str(concat!(
                "remove the comment or documentation attribute. The only comments allowed are an ",
                "SPDX license header on the first line, a `// SAFETY:` comment above unsafe code, ",
                "a `# Safety` section of an unsafe fn or trait and a comment that is one URL",
            )),
            Self::Document { allowed } => write!(
                formatter,
                "remove the document. {allowed} at the root of the repository is the only document allowed",
            ),
            Self::UnknownFileKind => formatter.write_str(
                "add the kind of this file to the file kinds of the source rules, or remove the file",
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub rule: Rule,
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub checked: usize,
}
