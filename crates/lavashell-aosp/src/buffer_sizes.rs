// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::PixelSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferSizes {
    default_lengths: (i32, i32),
    max: PixelSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BufferSizesError {
    #[error("the buffer sizes {default:?} and {max:?} exceed the signed 32-bit lengths of the NDK")]
    BeyondNdk { default: PixelSize, max: PixelSize },
    #[error(
        "the default size {default:?} of the buffers exceeds the maximum {max:?}, so every frame drawn at the \
         default size would be released without being drawn"
    )]
    DefaultBeyondMax { default: PixelSize, max: PixelSize },
}

impl BufferSizes {
    pub fn new(default: PixelSize, max: PixelSize) -> Result<Self, BufferSizesError> {
        let (Some(default_lengths), Some(_max_only_has_to_fit_the_ndk)) =
            (default.signed(), max.signed())
        else {
            return Err(BufferSizesError::BeyondNdk { default, max });
        };
        if default.width > max.width || default.height > max.height {
            return Err(BufferSizesError::DefaultBeyondMax { default, max });
        }
        Ok(Self {
            default_lengths,
            max,
        })
    }

    #[must_use]
    pub fn default_lengths(&self) -> (i32, i32) {
        self.default_lengths
    }

    #[must_use]
    pub fn max(&self) -> PixelSize {
        self.max
    }
}
