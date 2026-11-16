// SPDX-License-Identifier: GPL-3.0-only
use std::num::{NonZeroU32, ParseIntError};
use std::str::FromStr;

const BYTES_PER_KIB: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize {
    bytes: NonZeroU32,
}

#[derive(Debug, thiserror::Error)]
pub enum PageSizeError {
    #[error("the page size is not a positive whole number of bytes: {0}")]
    NotANumber(#[source] ParseIntError),
    #[error("the page size of {bytes} bytes is not a whole number of KiB")]
    NotWholeKib { bytes: NonZeroU32 },
}

impl FromStr for PageSize {
    type Err = PageSizeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes: NonZeroU32 = text.parse().map_err(PageSizeError::NotANumber)?;
        bytes
            .get()
            .is_multiple_of(BYTES_PER_KIB)
            .then_some(Self { bytes })
            .ok_or(PageSizeError::NotWholeKib { bytes })
    }
}

impl PageSize {
    fn kib(self) -> u32 {
        self.bytes.get() / BYTES_PER_KIB
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PackageAlignment {
    pub page_size: PageSize,
    pub file_alignment: u32,
}

impl PackageAlignment {
    #[must_use]
    pub fn zipalign_arguments(self) -> [String; 3] {
        [
            "-P".to_owned(),
            self.page_size.kib().to_string(),
            self.file_alignment.to_string(),
        ]
    }
}

#[cfg(test)]
mod tests;
