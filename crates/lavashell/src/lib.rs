// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

#[cfg(target_os = "android")]
pub use lavashell_android as android;
pub use lavashell_components as components;
pub use lavashell_core as core;
pub use lavashell_effect_quality as effect_quality;
pub use lavashell_keyboard as keyboard;
pub use lavashell_materials as materials;
pub use lavashell_renderer as renderer;
pub use lavashell_runtime::{Capability, SetupError, run};
#[cfg(target_os = "linux")]
pub use lavashell_wayland as wayland;

#[cfg(target_os = "linux")]
pub type Platform = lavashell_wayland::Wayland;

#[cfg(target_os = "linux")]
pub type RunError = lavashell_runtime::PlatformRunError<Platform>;

pub mod prelude {
    #[cfg(target_os = "linux")]
    pub use crate::Platform;
    #[cfg(target_os = "android")]
    pub use lavashell_android::{
        AndroidRenderer, BufferSizes, NativeWindow, StreamFormat, SurfaceStream,
    };
    pub use lavashell_components::{
        MeasuredRecycler, RecycledItem, Recycler, RecyclerLayout, RecyclerStats, Selection, UiExt,
    };
    pub use lavashell_core::panel::LayerLevel;
    pub use lavashell_core::prelude::*;
    pub use lavashell_renderer::{Fonts, Painter, VulkanRenderer};
    pub use lavashell_runtime::App;
    pub use lavashell_runtime::clipboard::{Clipboard, Error as ClipboardError};
    pub use lavashell_runtime::drag::Error as DragError;
    pub use lavashell_runtime::drag::{DragGesture, DragRequest, DropEvent};
    pub use lavashell_runtime::layer_surface::LayerSurface;
    pub use lavashell_runtime::observers::{HostKey, KeyboardFocus};
    pub use lavashell_runtime::pipe_transfers::Error as PipeTransferError;
    pub use lavashell_runtime::session_lock::SessionLock;
    pub use lavashell_runtime::shape::WindowShape;
    pub use lavashell_runtime::view_actions::ViewActions;
    pub use lavashell_runtime::watches::Error as WatchError;
    pub use lavashell_runtime::window::Window;
}
