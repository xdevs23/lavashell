// SPDX-License-Identifier: GPL-3.0-only
//! The native library of the AOSP emulator test of `SurfaceStream`. The test app sends frames of solid colors into the
//! Surface of a stream. This library renders each latched frame and reads its colors back for the test to compare.

#[cfg(target_os = "android")]
mod error;
#[cfg(target_os = "android")]
mod jni;
#[cfg(target_os = "android")]
mod stream_round_trip;
