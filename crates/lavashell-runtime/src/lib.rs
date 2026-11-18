// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod accessibility;
mod app;
mod builder_actor;
mod choreographer_actor;
pub mod clipboard;
mod controls_actor;
mod cpu_time;
mod deadline_receiver;
pub mod drag;
mod failures;
mod focus_grab;
mod input;
pub mod layer_surface;
mod lifecycle;
mod logging;
pub mod observers;
mod output_records;
mod panels;
pub mod pipe_transfers;
#[cfg(test)]
mod preference_reader;
mod registry;
mod renderer_actor;
mod run;
mod run_end;
mod scene_item;
pub mod scene_pipeline;
pub mod scheduler;
mod scheduler_actor;
pub mod session_lock;
mod settings;
pub mod shape;
mod shape_geometry;
mod single_precision;
mod surface_records;
pub mod surface_slots;
#[cfg(test)]
mod test_error;
#[cfg(test)]
mod test_host;
mod transfer_pipe;
pub mod view_actions;
mod view_actor;
pub mod watches;
pub mod window;
mod window_control;
mod window_requests;

pub use app::{App, SetupError};
pub use controls_actor::ControlsError;
pub use lavashell_platform::Capability;
pub use panels::{KeyboardPolicy, PanelOptions};
pub use renderer_actor::RendererError;
pub use run::{RunError, run, run_with_backend};
pub use view_actor::{Stopped, ViewError};
