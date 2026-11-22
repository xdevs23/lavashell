// SPDX-License-Identifier: GPL-3.0-only
use clap::ValueEnum;
use serde::Serialize;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ApiLevel(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ApiLevelMatch {
    Exact,
    AtLeast,
}

impl ApiLevelMatch {
    #[must_use]
    pub fn accepts(self, declared: ApiLevel, required: Option<ApiLevel>) -> bool {
        required.is_some_and(|required| match self {
            Self::Exact => declared == required,
            Self::AtLeast => declared >= required,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{text:?} is not an API level, which is written as decimal digits only")]
pub struct ApiLevelError {
    text: String,
}

impl FromStr for ApiLevel {
    type Err = ApiLevelError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let rejected = || ApiLevelError {
            text: text.to_owned(),
        };
        if !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(rejected());
        }
        text.parse().map(Self).map_err(|_overflow| rejected())
    }
}

impl fmt::Display for ApiLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
