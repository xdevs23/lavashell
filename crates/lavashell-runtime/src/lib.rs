// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod choreographers;
mod clipboard;
mod error;
mod input;
mod lifecycle;
mod output_records;
mod panels;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
mod scheduler;
mod session_lock;
mod settings;
mod shape;
mod surface_records;
#[cfg(test)]
mod test_error;
mod transfer_pipe;
mod view_actions;
mod watches;
mod window;
mod window_move;

pub use choreographers::{Choreographers, SceneSink};
pub use clipboard::{Clipboard, ClipboardTransfers, TextClipboard};
pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use panels::{
    ContentState, KeyboardPolicy, Panel, PanelContent, PanelOptions, PanelRecords, PanelsPerOutput,
    Transaction,
};
pub use run_end::RunEnd;
pub use scheduler::{
    FrameKind, FrameScheduler, Redraw, SchedulerInput, SchedulerOutput, ViewState,
};
pub use session_lock::{ActiveLock, LockConfiguration, LockStep, SessionLock};
pub use shape::WindowShape;
pub use surface_records::{SurfaceRecords, measure_content};
pub use transfer_pipe::{Progress, ReadBuffer, write_available};
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use watches::Watches;
pub use window::{DragRegion, Window};
