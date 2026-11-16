// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod choice;
mod draw_times;
#[cfg(test)]
mod test_draws;
mod variants;

pub use choice::{
    BudgetChooser, Choice, DrawId, EffectDraw, FrameBudget, FramePlan, Planned, QualityChooser,
    VariantOption,
};
pub use draw_times::{DrawTimeKey, DrawTimes, DrawnTimes, GpuTime};
pub use variants::{EffectId, OfferedVariant, VariantId, VariantQuality};
