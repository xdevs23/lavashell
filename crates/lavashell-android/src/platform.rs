// SPDX-License-Identifier: GPL-3.0-only
//! The renderer, ndk crate and hardware-buffer items used by the AOSP code in this crate. The host tests replace this
//! module with `platform/simulated.rs`, which defines the same names and simulates them.

pub(crate) use crate::hardware_buffer::{HardwareBufferMemory, PropertiesError};
pub(crate) use crate::presentation::vulkan_window;
pub(crate) use ::ndk;
pub(crate) use lavashell_renderer::{
    Completion, Error as ReleaseError, ForeignImport, ForeignImporter, SampledMemory, Sampling,
    VulkanRenderer,
};
