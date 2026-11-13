// SPDX-License-Identifier: GPL-3.0-only
use lavashell_renderer::{NoStreams, Painter, StreamBackend, VulkanRenderer};

/// Creates the streams on the renderer thread, using the renderer GPU.
pub type StreamSetup<S> =
    Box<dyn FnOnce(&<S as StreamBackend>::Gpu) -> Result<S, <S as StreamBackend>::Error> + Send>;

/// Runs on the renderer thread and receives the streams and the painter. The painter records a draw failure for each
/// stream frame that cannot be drawn.
pub type StreamTask<S> = Box<dyn FnOnce(&mut S, &Painter) + Send>;

/// The setup of a platform without streams.
#[must_use]
pub fn no_streams() -> StreamSetup<NoStreams> {
    Box::new(|_: &VulkanRenderer| Ok(NoStreams::default()))
}
