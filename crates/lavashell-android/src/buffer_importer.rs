// SPDX-License-Identifier: GPL-3.0-only
//! The import of the frames of streams into Vulkan, through the hardware buffers of the frames.

use crate::buffer_sampling;
use crate::error::Error;
use crate::platform::{
    Completion, ForeignImport, ForeignImporter, HardwareBufferMemory, SampledMemory, Sampling,
    VulkanRenderer,
};
use crate::surface_stream::QueuedFrame;
use lavashell_renderer::vulkan::Status;
use lavashell_renderer::{Completions, FrameImporter, StreamPixels};
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub(crate) struct HardwareBufferImporter(ForeignImporter);

impl HardwareBufferImporter {
    pub fn new(gpu: &VulkanRenderer) -> Result<Self, Error> {
        let importer = HardwareBufferMemory::importer(gpu).map_err(Error::Importer)?;
        Ok(Self(importer))
    }

    /// The format reported for `buffer` by the device of the importer.
    #[cfg(target_os = "android")]
    pub fn buffer_format(
        &self,
        buffer: &crate::platform::ndk::hardware_buffer::HardwareBuffer,
    ) -> Result<lavashell_renderer::ForeignFormat, Error> {
        crate::hardware_buffer::read_format(&self.0, buffer).map_err(Error::BufferFormat)
    }
}

impl Completions for HardwareBufferImporter {
    type Completion = Completion;
    type Error = Error;

    fn completion(&self) -> Result<Completion, Error> {
        self.0.completion().map_err(Error::SubmitCompletion)
    }

    fn status(&self, completion: &Completion) -> Result<Status, Error> {
        completion.status().map_err(Error::CompletionStatus)
    }
}

impl FrameImporter for HardwareBufferImporter {
    type Gpu = VulkanRenderer;
    type Frame = QueuedFrame;
    type Sampled = SampledMemory<HardwareBufferMemory>;
    type Import = ForeignImport;

    fn sample(&self, frame: &QueuedFrame) -> Result<Sampling<Self::Sampled>, Error> {
        buffer_sampling::sampling(&self.0, &frame.buffer).map_err(Error::BufferProperties)
    }

    fn import(&self, sampled: &Self::Sampled) -> Result<ForeignImport, Error> {
        self.0.import(sampled).map_err(Error::ImportBuffer)
    }

    fn acquire(
        &self,
        gpu: &mut VulkanRenderer,
        import: &Rc<ForeignImport>,
    ) -> Result<StreamPixels, Error> {
        self.0.acquire(gpu, import).map_err(Error::WrapImport)
    }

    fn release(
        &mut self,
        gpu: &mut VulkanRenderer,
        imports: &[Rc<ForeignImport>],
    ) -> Result<Option<OwnedFd>, Error> {
        let imports = imports.iter().map(Rc::as_ref);
        self.0.release(gpu, imports).map_err(Error::HandBack)
    }

    fn poll(&mut self) -> Result<(), Error> {
        self.0.poll().map_err(Error::RetireImports)
    }
}
