// SPDX-License-Identifier: GPL-3.0-only
//! The platform of an AOSP 12 or later host, and the streams of frames that other apps draw for it.
//! The renderer retires a Surface destroyed by the host before `AospHost::surface_destroyed` returns.

// On a host other than Android, the crate builds only for the tests, which run against a simulated platform.
#![cfg(any(target_os = "android", test))]

#[cfg(target_os = "android")]
mod aosp;
mod buffer_importer;
mod buffer_sampling;
mod error;
mod fenced_image;
#[cfg(target_os = "android")]
mod hardware_buffer;
mod host;
mod native_window;
#[cfg_attr(not(target_os = "android"), path = "platform/simulated.rs")]
mod platform;
#[cfg(target_os = "android")]
mod presentation;
mod streams;
#[cfg(target_os = "android")]
mod surface_jni;
mod surface_stream;

#[cfg(target_os = "android")]
pub use aosp::Aosp;
pub use error::Error;
#[cfg(target_os = "android")]
pub use hardware_buffer::PropertiesError;
pub use host::{AospError, AospHost, RunEnded};
pub use native_window::NativeWindow;
#[cfg(target_os = "android")]
pub use presentation::AospPresentation;
pub use streams::AospStreams;
#[cfg(target_os = "android")]
pub use surface_jni::{surface_of_window, window_of_surface};
pub use surface_stream::{BufferSizes, StreamFormat, SurfaceStream};

#[cfg(all(test, not(target_os = "android")))]
mod tests;
