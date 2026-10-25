// SPDX-License-Identifier: GPL-3.0-only
//! Chooses a quality variant for each effect drawn in a frame, such as a fine or a coarse backdrop blur. The choice
//! uses the GPU times measured for recent draws of each variant, so that all effects of the frame fit into the GPU time
//! that the host sets aside for them.
#![forbid(unsafe_code)]

mod choice;
mod draw_times;
mod variants;

pub use choice::{
    BudgetChooser, Choice, DrawId, EffectDraw, FrameBudget, FramePlan, Planned, QualityChooser,
    VariantOption,
};
pub use draw_times::{DrawTimeKey, DrawTimes, DrawnTimes, GpuTime};
pub use variants::{EffectId, VariantId};
