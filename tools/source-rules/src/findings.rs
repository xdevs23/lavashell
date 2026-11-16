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
    PythonOutsidePlace { directory: &'static str },
    InexactWorkspaceDependency { name: String },
    UnlistedMember { member: String },
    OwnDependency { name: String },
    OwnVersion,
    OwnEdition,
    OwnLints,
    NoCopyCatalogs,
    CopyColumns,
    DuplicateCopyKey { key: String },
    EmptyCopy { key: String },
    CopyPunctuation { key: String },
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
            Self::PythonOutsidePlace { directory } => write!(
                formatter,
                "the source rules allow Python files only directly in {directory}. Move the file there or rewrite it in Rust",
            ),
            Self::InexactWorkspaceDependency { name } => write!(
                formatter,
                "lock the workspace dependency {name} to an exact version `=x.y.z` or give it a path",
            ),
            Self::UnlistedMember { member } => write!(
                formatter,
                "add the member {member} to the workspace dependencies as a path entry",
            ),
            Self::OwnDependency { name } => write!(
                formatter,
                "take the dependency {name} from the workspace with `{name}.workspace = true`",
            ),
            Self::OwnVersion => formatter
                .write_str("take package.version from the workspace with `version.workspace = true`"),
            Self::OwnEdition => formatter
                .write_str("take package.edition from the workspace with `edition.workspace = true`"),
            Self::OwnLints => formatter
                .write_str("take the lints from the workspace with `[lints]` and `workspace = true`"),
            Self::NoCopyCatalogs => formatter.write_str(
                "the copy rules found no catalog at examples/<example>/copy/<area>.en.tsv",
            ),
            Self::CopyColumns => {
                formatter.write_str("give the copy row a key and a value, separated by one tab")
            }
            Self::DuplicateCopyKey { key } => {
                write!(formatter, "give the copy key {key} to one row only")
            }
            Self::EmptyCopy { key } => write!(formatter, "write the copy of the key {key}"),
            Self::CopyPunctuation { key } => write!(
                formatter,
                "remove the semicolon, em dash or en dash from the copy of the key {key}",
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
