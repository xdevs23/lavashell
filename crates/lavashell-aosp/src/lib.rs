// SPDX-License-Identifier: GPL-3.0-only

#![cfg(any(target_os = "android", test))]

#[cfg(target_os = "android")]
mod aosp;
mod buffer_importer;
mod buffer_sampling;
mod buffer_sizes;
mod error;
mod fenced_image;
#[cfg(target_os = "android")]
mod hardware_buffer;
mod host;
#[cfg(target_os = "android")]
mod host_jni;
mod input;
mod layout_size;
#[cfg(target_os = "android")]
mod motion_jni;
mod native_window;
mod output;
#[cfg_attr(not(target_os = "android"), path = "platform/simulated.rs")]
mod platform;
#[cfg(target_os = "android")]
mod presentation;
mod settings;
mod streams;
#[cfg(target_os = "android")]
mod surface_jni;
mod surface_stream;

#[cfg(target_os = "android")]
pub use aosp::Aosp;
pub use buffer_sizes::{BufferSizes, BufferSizesError};
pub use error::Error;
#[cfg(target_os = "android")]
pub use hardware_buffer::{HardwareBufferMemory, PropertiesError};
pub use host::{AospError, AospHost, HostLink, RunEnded};
#[cfg(target_os = "android")]
pub use host_jni::{configuration_of, display_of};
pub use input::{
    HistoricalPositions, MotionEvent, PixelPosition, TouchAction, TouchPointer, TouchPositions,
};
pub use layout_size::LayoutSize;
#[cfg(target_os = "android")]
pub use motion_jni::{MalformedMotion, motion_event_of};
pub use native_window::NativeWindow;
pub use output::HostDisplay;
#[cfg(target_os = "android")]
pub use presentation::AospPresentation;
pub use settings::HostConfiguration;
pub use streams::AospStreams;
#[cfg(target_os = "android")]
pub use surface_jni::{surface_of_window, window_of_surface};
pub use surface_stream::{StreamFormat, SurfaceStream};

#[cfg(all(test, not(target_os = "android")))]
mod tests;
