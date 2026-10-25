// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::vulkan::{GpuProfiler, SampleTag};
use skia_safe::{Canvas, gpu::DirectContext};
use std::cell::RefCell;

/// Measures the GPU time of single draws on the GPU context of a canvas, without the work before and after each
/// draw.
pub(super) struct GpuSpan<'a> {
    context: DirectContext,
    profiler: &'a RefCell<GpuProfiler>,
}

impl<'a> GpuSpan<'a> {
    pub(super) fn new(canvas: &Canvas, profiler: &'a RefCell<GpuProfiler>) -> Option<Self> {
        let context = canvas.direct_context()?;
        Some(Self { context, profiler })
    }

    /// Runs `draw` once and flushes the GPU work before and after it, so that the sample measures only `draw`. The
    /// flushes split the render pass around `draw`, and the sample includes the cost of that split.
    ///
    /// # Errors
    /// Fails when the GPU timer cannot start or stop. No sample is submitted then, but `draw` runs either way.
    pub(super) fn time(&mut self, tag: SampleTag, draw: impl FnOnce()) -> Result<(), Error> {
        flush_and_submit(&mut self.context);
        let token = self.profiler.borrow_mut().begin(tag);
        draw();
        let token = token?;
        flush_and_submit(&mut self.context);
        self.profiler.borrow_mut().end(token)
    }
}

fn flush_and_submit(context: &mut DirectContext) {
    let _chained_context = context.flush_and_submit();
}
