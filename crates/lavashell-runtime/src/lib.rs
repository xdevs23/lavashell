// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what its host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod choreographers;
mod clipboard;
mod drag;
mod error;
mod input;
mod lifecycle;
mod output_records;
mod pipe_transfers;
#[cfg(test)]
mod preference_reader;
mod run_end;
mod scene_item;
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
pub use drag::{DragGesture, DragRequest, Drags, DropEvent};
pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use run_end::RunEnd;
pub use shape::WindowShape;
pub use surface_records::{SurfaceRecords, measure_content};
pub use view_actions::{ActionBatch, ActionQueue, ViewActions};
pub use watches::Watches;
pub use window::{DragRegion, Window};
