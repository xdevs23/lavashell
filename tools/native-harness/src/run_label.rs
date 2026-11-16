// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;
use std::str::FromStr;

const SEPARATORS: [char; 2] = ['-', '_'];

#[derive(Clone, Debug)]
pub struct RunLabel(String);

#[derive(Debug, thiserror::Error)]
#[error("{0:?} must be ASCII letters, digits, '-' and '_'")]
pub struct InvalidRunLabel(String);

impl FromStr for RunLabel {
    type Err = InvalidRunLabel;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let allowed =
            |character: char| character.is_ascii_alphanumeric() || SEPARATORS.contains(&character);
        if !text.is_empty() && text.chars().all(allowed) {
            Ok(Self(text.to_owned()))
        } else {
            Err(InvalidRunLabel(text.to_owned()))
        }
    }
}

impl fmt::Display for RunLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests;
