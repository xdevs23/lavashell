// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

pub use lavashell_components as components;
pub use lavashell_core as core;
pub use lavashell_effect_quality as effect_quality;
pub use lavashell_enum_values as enum_values;
pub use lavashell_error_chain as error_chain;
pub use lavashell_keyboard as keyboard;
pub use lavashell_materials as materials;
pub use lavashell_renderer as renderer;
pub use lavashell_runtime::{Capability, RunError as PlatformRunError, SetupError, run};
pub use lavashell_vulkan_renderer as vulkan_renderer;

#[cfg_attr(target_os = "android", path = "selected/aosp.rs")]
#[cfg_attr(target_os = "linux", path = "selected/wayland.rs")]
#[cfg_attr(
    not(any(target_os = "android", target_os = "linux")),
    path = "selected/unsupported.rs"
)]
mod selected;

pub use selected::*;

pub type RunError = lavashell_runtime::RunError<Platform>;

pub mod prelude {
    pub use crate::Platform;
    pub use lavashell_components::{
        MeasuredRecycler, RecycledItem, Recycler, RecyclerLayout, RecyclerStats, Selection, UiExt,
    };
    pub use lavashell_core::panel::LayerLevel;
    pub use lavashell_core::prelude::*;
    pub use lavashell_renderer::{Fonts, Painter};
    pub use lavashell_runtime::App;
    pub use lavashell_runtime::clipboard::{Clipboard, Error as ClipboardError};
    pub use lavashell_runtime::drag::Error as DragError;
    pub use lavashell_runtime::drag::{DragGesture, DragRequest, DropEvent};
    pub use lavashell_runtime::layer_surface::LayerSurface;
    pub use lavashell_runtime::observers::{HostKey, KeyAction, KeyboardFocus};
    pub use lavashell_runtime::pipe_transfers::Error as PipeTransferError;
    pub use lavashell_runtime::session_lock::SessionLock;
    pub use lavashell_runtime::shape::WindowShape;
    pub use lavashell_runtime::view_actions::ViewActions;
    pub use lavashell_runtime::watches::Error as WatchError;
    pub use lavashell_runtime::window::Window;
    pub use lavashell_vulkan_renderer::VulkanRenderer;
}
