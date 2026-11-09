// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod accessibility;
mod builder_actor;
mod choreographer_actor;
mod clipboard;
mod drag;
mod error;
mod focus_grab;
mod input;
mod lifecycle;
mod output_records;
mod panels;
mod pipe_transfers;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
mod scene_pipeline;
mod scheduler;
mod session_lock;
mod settings;
mod shape;
mod single_precision;
mod surface_records;
mod surface_slots;
#[cfg(test)]
mod test_error;
mod transfer_pipe;
mod view_actions;
mod watches;
mod window;
mod window_move;

pub use clipboard::{Clipboard, ClipboardTransfers, TextClipboard};
pub use drag::{DragGesture, DragRequest, Drags, DropEvent};
pub use error::{AccessibilityFailure, Error};
pub use focus_grab::FocusGrabs;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use panels::{
    ContentState, KeyboardPolicy, Panel, PanelContent, PanelOptions, PanelRecords, PanelUpdate,
    PanelsPerOutput, Transaction,
};
pub use run_end::RunEnd;
pub use scene_pipeline::{BuilderObserver, BuilderReport, ScenePipeline, SceneSink};
pub use scheduler::{
    FrameKind, FrameScheduler, FrameSequence, Redraw, SchedulerInput, SchedulerOutput, ViewState,
};
pub use session_lock::{ActiveLock, LockConfiguration, LockStep, SessionLock};
pub use shape::{SurfaceShape, WindowShape};
pub use surface_records::{AccessibilityOutcome, SurfaceRecords, measure_content};
pub use surface_slots::BuiltSurface;
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use watches::Watches;
pub use window::{DragRegion, Window};
