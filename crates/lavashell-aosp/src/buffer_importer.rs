// SPDX-License-Identifier: GPL-3.0-only

use crate::buffer_sampling;
use crate::error::Error;
use crate::platform::{
    Completion, ForeignImport, ForeignImporter, HardwareBufferMemory, SampledMemory, Sampling,
    VulkanRenderer,
};
use crate::surface_stream::QueuedFrame;
use lavashell_renderer::{
    CompletionStatus, Completions, FrameImporter, FrameSampler, ImportHandBack, StreamPixels,
};
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub(crate) struct HardwareBufferImporter(pub(crate) ForeignImporter);

impl HardwareBufferImporter {
    pub fn new(gpu: &VulkanRenderer) -> Result<Self, Error> {
        let importer = HardwareBufferMemory::importer(gpu).map_err(Error::Importer)?;
        Ok(Self(importer))
    }
}

impl Completions for HardwareBufferImporter {
    type Completion = Completion;
    type Error = Error;

    fn completion(&self) -> Result<Completion, Error> {
        self.0.completion().map_err(Error::SubmitCompletion)
    }

    fn status(&self, completion: &Completion) -> Result<CompletionStatus, Error> {
        completion.status().map_err(Error::CompletionStatus)
    }
}

impl FrameSampler for HardwareBufferImporter {
    type Frame = QueuedFrame;
    type Sampled = SampledMemory<HardwareBufferMemory>;

    fn sample(&self, frame: &QueuedFrame) -> Result<Sampling<Self::Sampled>, Error> {
        buffer_sampling::read_sampling(&self.0, &frame.buffer).map_err(Error::BufferProperties)
    }
}

impl FrameImporter for HardwareBufferImporter {
    type Gpu = VulkanRenderer;
    type Import = ForeignImport;

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
}

impl ImportHandBack for HardwareBufferImporter {
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
