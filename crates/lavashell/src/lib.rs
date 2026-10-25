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
    pub use lavashell_wayland::{
        App, Clipboard, DragEvent, DragGesture, DragOfferInfo, DragRequest, DropEvent, FrameReport,
        FrameStage, LayerLevel, LayerSurface, TransferId, ViewActions, Window, WindowShape,
    };
}
