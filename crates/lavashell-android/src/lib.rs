// SPDX-License-Identifier: GPL-3.0-only
//! Vulkan rendering for an AOSP 12 or later host.
//! Render and detach the renderer on one thread, and detach it before its surface is destroyed.

// On a host other than Android, the crate builds only for the tests, which run against a simulated platform.
#![cfg(any(target_os = "android", test))]

mod buffer_sampling;
mod error;
mod fenced_image;
#[cfg(target_os = "android")]
mod hardware_buffer;
mod latched_frames;
mod native_window;
#[cfg_attr(not(target_os = "android"), path = "platform/simulated.rs")]
mod platform;
mod renderer;
#[cfg(target_os = "android")]
mod surface_jni;
mod surface_stream;

pub use error::Error;
#[cfg(target_os = "android")]
pub use hardware_buffer::PropertiesError;
pub use latched_frames::Latch;
pub use native_window::NativeWindow;
pub use renderer::AndroidRenderer;
#[cfg(target_os = "android")]
pub use surface_jni::{surface_of_window, window_of_surface};
pub use surface_stream::{BufferSizes, StreamFormat, SurfaceStream};

#[cfg(all(test, not(target_os = "android")))]
mod tests;
