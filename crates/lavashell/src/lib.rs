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
#[cfg(target_os = "linux")]
pub use lavashell_wayland as wayland;

pub mod prelude {
    #[cfg(target_os = "android")]
    pub use lavashell_android::{
        AndroidRenderer, BufferSizes, NativeWindow, StreamFormat, SurfaceStream,
    };
    pub use lavashell_components::{
        MeasuredRecycler, RecycledItem, Recycler, RecyclerLayout, RecyclerStats, Selection, UiExt,
    };
    pub use lavashell_core::prelude::*;
    pub use lavashell_renderer::{Fonts, Painter, VulkanRenderer};
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::clipboard::{Clipboard, Error as ClipboardError};
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::drag::Error as DragError;
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::drag::{DragGesture, DragRequest, DropEvent};
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::pipe_transfers::Error as PipeTransferError;
    pub use lavashell_runtime::shape::WindowShape;
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::view_actions::ViewActions;
    #[cfg(target_os = "linux")]
    pub use lavashell_runtime::watches::Error as WatchError;
    pub use lavashell_runtime::window::Window;
    #[cfg(target_os = "linux")]
    pub use lavashell_wayland::{App, LayerLevel, LayerSurface};
}
