// SPDX-License-Identifier: GPL-3.0-only
use lavashell_platform::{Backend, Platform, SurfaceAttachment};
use lavashell_vulkan_renderer::{RendererBackend, VulkanWindow};

pub trait RenderedPlatform: Platform<Backend: RendererBackend> {}

impl<P: Platform<Backend: RendererBackend>> RenderedPlatform for P {}

pub(crate) type PlatformStreams<P> = <<P as Platform>::Backend as Backend>::Streams;

pub(crate) type WindowAttachment<C> = SurfaceAttachment<C, VulkanWindow>;
