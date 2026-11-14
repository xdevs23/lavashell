// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what the host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod accessibility;
mod app;
mod builder_actor;
mod choreographer_actor;
pub mod clipboard;
mod cpu_time;
pub mod deadline_receiver;
pub mod drag;
mod failures;
pub mod focus_grab;
pub mod input;
pub mod layer_surface;
mod lifecycle;
pub mod observers;
mod output_records;
pub mod panels;
pub mod pipe_transfers;
#[cfg(test)]
mod preference_reader;
mod registry;
pub mod renderer_actor;
mod run;
pub mod run_end;
mod scene_item;
pub mod scene_pipeline;
pub mod scheduler;
pub mod scheduler_actor;
pub mod session_lock;
mod settings;
pub mod shape;
pub mod shape_geometry;
mod single_precision;
mod standard_error;
pub mod surface_records;
pub mod surface_slots;
#[cfg(test)]
mod test_error;
#[cfg(test)]
mod test_host;
mod transfer_pipe;
pub mod view_actions;
pub mod view_actor;
pub mod watches;
pub mod window;
mod window_move;

pub use app::{App, SetupError};
pub use lavashell_platform::Capability;
pub use run::{RunError, run};
