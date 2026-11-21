// SPDX-License-Identifier: GPL-3.0-only
use std::any::Any;
use std::fmt;

#[derive(Debug)]
pub enum PanicValue {
    Text(String),
    NotText,
}

impl PanicValue {
    pub(crate) fn of(payload: &(dyn Any + Send)) -> Self {
        (payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned()))
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .map_or(Self::NotText, Self::Text)
    }
}

impl fmt::Display for PanicValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => write!(formatter, "\"{text}\""),
            Self::NotText => formatter.write_str("a value that is not text"),
        }
    }
}
