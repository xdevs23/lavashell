// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares. Each platform reports to them what the host system does and executes the
//! requests that follow.
#![forbid(unsafe_code)]

mod accessibility;
mod builder_actor;
mod choreographer_actor;
mod clipboard;
mod cpu_time;
mod deadline_receiver;
mod drag;
mod error;
mod failures;
mod focus_grab;
mod input;
mod layer_surface;
mod lifecycle;
mod observers;
mod output_records;
mod panels;
mod pipe_transfers;
#[cfg(test)]
mod preference_reader;
mod renderer_actor;
mod run;
mod run_end;
mod scene_item;
mod scene_pipeline;
mod scheduler;
mod scheduler_actor;
mod session_lock;
mod settings;
mod shape;
mod shape_geometry;
mod single_precision;
mod surface_records;
mod surface_slots;
#[cfg(test)]
mod test_error;
#[cfg(test)]
mod test_host;
mod transfer_pipe;
mod view_actions;
mod view_actor;
mod watches;
mod window;
mod window_move;

pub use clipboard::{Clipboard, ClipboardTransfers};
pub use drag::{DragGesture, DragRequest, Drags, DropEvent};
pub use error::{AccessibilityFailure, Error};
pub use focus_grab::FocusGrabs;
pub use input::InputFilter;
pub use layer_surface::LayerSurface;
pub use observers::{HostKey, KeyboardFocus, Observers};
pub use panels::{
    ContentState, KeyboardPolicy, Panel, PanelContent, PanelOptions, PanelRecords, PanelRequests,
    PanelUpdate, PanelsPerOutput, Transaction,
};
pub use renderer_actor::{
    Rendered, Renderer, RendererError, RendererInput, RendererObserver, RendererOutput,
    RendererStopped,
};
pub use run::{RunError, RuntimeSetup, SurfaceSetup, drive};
pub use run_end::RunEnd;
pub use scene_pipeline::{BuilderObserver, BuilderReport, ScenePipeline, SceneSink};
pub use scheduler::{FrameKind, FrameSequence, Redraw, SchedulerInput, SchedulerOutput, ViewState};
pub use scheduler_actor::SchedulerObserver;
pub use session_lock::{LockConfiguration, SessionLock};
pub use shape::{SurfaceShape, WindowShape};
pub use shape_geometry::ShapeGeometryError;
pub use surface_records::{AccessibilityOutcome, SurfaceRecords, measure_content};
pub use surface_slots::BuiltSurface;
pub use view_actions::{ActionQueue, ViewActions};
pub use view_actor::{Stopped, ViewError};
pub use watches::Watches;
pub use window::{DragRegion, Window};

use clipboard::TextClipboard;
use cpu_time::CpuStamp;
use input::{InputRouter, RoutedInput};
use output_records::OutputRecords;
use scheduler_actor::SchedulerThread;
use session_lock::{ActiveLock, LockStep};
use shape_geometry::ShapeGeometry;
use view_actions::ActionBatch;
