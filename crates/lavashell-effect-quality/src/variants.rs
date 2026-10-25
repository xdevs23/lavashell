// SPDX-License-Identifier: GPL-3.0-only

pub use lavashell_core::effects::EffectId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantId(pub u64);
