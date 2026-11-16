// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "android")]
mod error;
#[cfg(target_os = "android")]
mod jni;
#[cfg(target_os = "android")]
mod pane;
#[cfg(target_os = "android")]
mod round_trips;
#[cfg(target_os = "android")]
mod stream_round_trip;
#[cfg(any(target_os = "android", test))]
mod ycbcr;
