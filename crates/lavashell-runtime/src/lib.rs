// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod accessibility;
mod choreographers;
mod error;
mod input;
mod lifecycle;
mod output_records;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
mod settings;
mod shape;
mod single_precision;
mod surface_records;
#[cfg(test)]
mod test_error;
mod view_actions;
mod window;
mod window_move;

pub use choreographers::{Choreographers, SceneSink};
pub use error::{AccessibilityFailure, Error};
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use run_end::RunEnd;
pub use shape::WindowShape;
pub use surface_records::{AccessibilityOutcome, SurfaceRecords, measure_content};
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use window::{DragRegion, Window};
