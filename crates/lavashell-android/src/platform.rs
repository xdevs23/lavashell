// SPDX-License-Identifier: GPL-3.0-only
//! The items of the renderer and of the ndk crate called by the AOSP code in this crate. The tests on the host replace
//! this module with `platform/simulated.rs`, which defines the same names with the same signatures.

pub(crate) use ::ndk;
pub(crate) use lavashell_renderer::{
    Completion, Error as ReleaseError, HardwareBufferImport, HardwareBufferImporter,
    HardwareBufferProperties, Sampling, SurfaceTarget, VulkanRenderer,
};
