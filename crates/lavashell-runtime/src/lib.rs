// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod builder_actor;
mod choreographer_actor;
mod error;
mod input;
mod lifecycle;
mod output_records;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
mod scene_pipeline;
mod scheduler;
mod settings;
mod shape;
mod surface_records;
mod surface_slots;
#[cfg(test)]
mod test_error;
mod view_actions;
mod window;
mod window_move;

pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use run_end::RunEnd;
pub use scene_pipeline::{BuilderObserver, BuilderReport, ScenePipeline, SceneSink};
pub use scheduler::{
    FrameKind, FrameScheduler, FrameSequence, Redraw, SchedulerInput, SchedulerOutput, ViewState,
};
pub use shape::WindowShape;
pub use surface_records::{SurfaceRecords, measure_content};
pub use surface_slots::BuiltScene;
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use window::{DragRegion, Window};
