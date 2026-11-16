// SPDX-License-Identifier: GPL-3.0-only
use std::cmp::Reverse;

pub use lavashell_core::effects::EffectId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantQuality(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfferedVariant {
    pub variant: VariantId,
    pub quality: VariantQuality,
}

impl OfferedVariant {
    pub(crate) fn preference(&self) -> (VariantQuality, Reverse<VariantId>) {
        (self.quality, Reverse(self.variant))
    }

    #[must_use]
    pub fn most_preferred(offered: &[Self]) -> Option<Self> {
        offered.iter().copied().max_by_key(Self::preference)
    }
}
