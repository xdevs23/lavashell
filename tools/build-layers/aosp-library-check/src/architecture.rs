// SPDX-License-Identifier: GPL-3.0-only
use clap::ValueEnum;
use object::elf;
use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Architecture {
    #[value(name = "aarch64")]
    Aarch64,
    #[value(name = "x86_64")]
    X86_64,
}

impl Architecture {
    #[must_use]
    pub fn machine(self) -> elf::Machine {
        match self {
            Self::Aarch64 => elf::EM_AARCH64,
            Self::X86_64 => elf::EM_X86_64,
        }
    }
}

impl fmt::Display for Architecture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_possible_value() {
            Some(value) => formatter.write_str(value.get_name()),
            None => Err(fmt::Error),
        }
    }
}

impl Serialize for Architecture {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
