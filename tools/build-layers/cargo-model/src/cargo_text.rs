// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};

pub trait ReadByCargo: Sized {
    fn read(text: &str) -> Option<Self>;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "String", untagged)]
pub enum CargoText<T: ReadByCargo> {
    Read(T),
    Unreadable(UnreadableText),
}

impl<T: ReadByCargo> From<String> for CargoText<T> {
    fn from(text: String) -> Self {
        T::read(&text).map_or(Self::Unreadable(UnreadableText(text)), Self::Read)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct UnreadableText(String);
