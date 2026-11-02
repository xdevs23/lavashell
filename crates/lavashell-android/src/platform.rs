// SPDX-License-Identifier: GPL-3.0-only
//! The items of the renderer, of the ndk crate and of the hardware-buffer memory called by the AOSP code in this
//! crate. The tests on the host replace this module with `platform/simulated.rs`, which defines the names used by
//! this crate and accepts the calls that this crate makes.

pub(crate) use crate::hardware_buffer::{HardwareBufferMemory, PropertiesError};
pub(crate) use ::ndk;
pub(crate) use lavashell_renderer::{
    Completion, Error as ReleaseError, ForeignImport, ForeignImporter, SampledMemory, Sampling,
    SurfaceTarget, VulkanRenderer,
};
