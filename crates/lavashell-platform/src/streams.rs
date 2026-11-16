// SPDX-License-Identifier: GPL-3.0-only
use lavashell_renderer::{NoStreams, Painter, StreamBackend, VulkanRenderer};

pub type StreamSetup<S> =
    Box<dyn FnOnce(&<S as StreamBackend>::Gpu) -> Result<S, <S as StreamBackend>::Error> + Send>;

pub type StreamTask<S> = Box<dyn FnOnce(&mut S, &Painter) + Send>;

#[must_use]
pub fn no_streams() -> StreamSetup<NoStreams> {
    Box::new(|_: &VulkanRenderer| Ok(NoStreams::default()))
}
