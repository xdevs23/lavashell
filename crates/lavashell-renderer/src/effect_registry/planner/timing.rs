// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::gpu_timing::RunningSample;
use crate::vulkan::{GpuProfiler, SampleTag};
use skia_safe::{Canvas, gpu::DirectContext};

pub(super) struct GpuSpan<'a> {
    context: DirectContext,
    profiler: &'a mut GpuProfiler,
}

impl<'a> GpuSpan<'a> {
    pub(super) fn new(canvas: &Canvas, profiler: &'a mut GpuProfiler) -> Option<Self> {
        let context = canvas.direct_context()?;
        Some(Self { context, profiler })
    }

    pub(super) fn time(&mut self, tag: SampleTag, draw: impl FnOnce()) -> Result<(), Error> {
        let sample = RunningSample::begin(&mut self.context, self.profiler, tag);
        draw();
        sample.end(&mut self.context, self.profiler)
    }
}
