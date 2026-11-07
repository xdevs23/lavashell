// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod builder_actor;
mod choreographer_actor;
mod cpu_time;
mod error;
mod input;
mod lifecycle;
mod output_records;
mod panels;
#[cfg(test)]
mod preference_reader;
mod renderer_actor;
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

pub use cpu_time::CpuStamp;
pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use panels::{
    ContentState, KeyboardPolicy, Panel, PanelContent, PanelOptions, PanelRecords, PanelRequests,
    PanelUpdate, PanelsPerOutput, Transaction,
};
pub use renderer_actor::{
    Rendered, Renderer, RendererError, RendererInput, RendererObserver, RendererOutput,
    RendererStopped,
};
pub use run_end::RunEnd;
pub use scene_pipeline::{
    BuilderObserver, BuilderReport, BuiltChange, BuiltScene, ScenePipeline, SceneSink,
};
pub use scheduler::{
    FrameKind, FrameScheduler, FrameSequence, Redraw, SchedulerInput, SchedulerOutput, ViewState,
};
pub use shape::{SurfaceShape, WindowShape};
pub use surface_records::{SurfaceRecords, measure_content};
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use window::{DragRegion, Window};
