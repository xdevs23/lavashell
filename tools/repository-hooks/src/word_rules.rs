// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::repository::Repository;
use regex::bytes::{Regex, RegexBuilder};
use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::Path;

const LISTED_WORDS: &str = "info/trademarks";
const EXEMPTED_SPELLINGS: &str = "info/trademark-spellings";

pub struct ListedLine {
    pub number: usize,
    pub text: String,
}

pub struct WordRules {
    listed: Regex,
    exempted: Option<Regex>,
}

impl WordRules {
    pub fn read(repository: &Repository) -> Result<Self, Error> {
        let common_directory = repository.common_directory()?;
        let listed_path = common_directory.join(LISTED_WORDS);
        let exempted_path = common_directory.join(EXEMPTED_SPELLINGS);
        let listed = read_alternation(&listed_path)?.ok_or_else(|| Error::NoListedWords {
            path: listed_path.clone(),
        })?;
        Ok(Self {
            listed: compile(
                &listed_path,
                RegexBuilder::new(&listed).case_insensitive(true),
            )?,
            exempted: read_alternation(&exempted_path)?
                .map(|exempted| compile(&exempted_path, &RegexBuilder::new(&exempted)))
                .transpose()?,
        })
    }

    pub fn contains_listed_word(&self, text: &[u8]) -> bool {
        let unexempted = self
            .exempted
            .as_ref()
            .map_or(Cow::Borrowed(text), |exempted| {
                exempted.replace_all(text, b"")
            });
        self.listed.is_match(&unexempted)
    }

    pub fn listed_lines(&self, content: &[u8]) -> Vec<ListedLine> {
        let binary = content.contains(&b'\0');
        content
            .split(|byte| *byte == b'\n')
            .zip(1..)
            .filter(|(line, _number)| !binary && self.contains_listed_word(line))
            .map(|(line, number)| ListedLine {
                number,
                text: String::from_utf8_lossy(line).trim().to_owned(),
            })
            .collect()
    }
}

fn read_alternation(path: &Path) -> Result<Option<String>, Error> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::ReadWordList {
                path: path.to_owned(),
                source,
            });
        }
    };
    let patterns: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    Ok((!patterns.is_empty()).then(|| patterns.join("|")))
}

fn compile(path: &Path, alternation: &RegexBuilder) -> Result<Regex, Error> {
    alternation
        .build()
        .map_err(|source| Error::CompileWordList {
            path: path.to_owned(),
            source,
        })
}
