// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod choreographers;
mod error;
mod input;
mod lifecycle;
mod output_records;
mod panels;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
mod settings;
mod shape;
mod surface_records;
#[cfg(test)]
mod test_error;
mod view_actions;
mod window;

pub use choreographers::{Choreographers, SceneSink};
pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use panels::{
    ContentState, KeyboardPolicy, Panel, PanelContent, PanelOptions, PanelRecords, PanelUpdate,
    PanelsPerOutput, Transaction,
};
pub use run_end::RunEnd;
pub use shape::{SurfaceShape, WindowShape};
pub use surface_records::{SurfaceRecords, measure_content};
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use window::{DragRegion, Window};
