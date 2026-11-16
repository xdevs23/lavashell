// SPDX-License-Identifier: GPL-3.0-only
use lavashell_effect_quality::{OfferedVariant, VariantId, VariantQuality};

pub(crate) fn only_variant(id: u64) -> Vec<OfferedVariant> {
    vec![OfferedVariant {
        variant: VariantId(id),
        quality: VariantQuality(0),
    }]
}
