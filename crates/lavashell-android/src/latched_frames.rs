// SPDX-License-Identifier: GPL-3.0-only
//! The frames latched from streams and their import into Vulkan. A latched frame is imported only when a paint first
//! draws it. Each frame returns to its producer once the GPU has finished every render that could read it.

use crate::error::{Error, Failures};
use crate::platform::{
    Completion, HardwareBufferImport, HardwareBufferImporter, HardwareBufferProperties, Sampling,
    VulkanRenderer,
};
use crate::surface_stream::{Acquired, BufferId, QueuedFrame, ReaderToken, SurfaceStream};
use lavashell_core::prelude::{DrawCommand, Presented, StreamId, StreamImage};
use lavashell_renderer::vulkan::Status;
use lavashell_renderer::{Painter, StreamFrames, StreamRejection};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::OwnedFd;
use std::rc::Rc;

/// The result of a latch on a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latch {
    /// A new frame was latched. Drawing this image draws that frame.
    Frame(StreamImage),
    /// The producer has queued no frame since the last latch, or the producer is still writing the queued frame.
    Unchanged,
    /// The newest frame was released without being drawn. Renders draw the frame latched before.
    Rejected(StreamRejection),
}

/// A frame latched from a stream. `import` is set once a paint has drawn the frame.
struct Latched {
    image: StreamImage,
    frame: QueuedFrame,
    properties: HardwareBufferProperties,
    import: Option<Rc<HardwareBufferImport>>,
}

/// The Vulkan import of a buffer. Later frames in the same buffer reuse the import while the reader that delivered
/// the buffer exists.
struct CachedImport {
    reader: ReaderToken,
    import: Rc<HardwareBufferImport>,
}

#[derive(Clone)]
enum Work {
    Submitted(Rc<Completion>),
    /// The device failed to submit a fence for the work. The work is known to be finished only at teardown, once
    /// the device is idle.
    Unknown,
}

enum ReleaseAfter {
    /// The sync fence exported after the last render. `None` when the work of that render had already finished.
    Fence(Option<OwnedFd>),
    /// The work of the last render, used after the export of the fence failed.
    Work(Work),
}

/// A frame that returns to the producer once the GPU has finished `work`. `_frame` is `None` when only the
/// completion of the work is held. A `Completion` dropped before the work has finished blocks until the GPU
/// finishes, so the completion is dropped only once the work is finished.
struct Held {
    work: Work,
    _frame: Option<QueuedFrame>,
}

pub(crate) struct LatchedFrames {
    frames: StreamFrames,
    latched: BTreeMap<StreamId, Latched>,
    imports: BTreeMap<BufferId, CachedImport>,
    importer: HardwareBufferImporter,
    release: ReleaseAfter,
    held: Vec<Held>,
}

impl LatchedFrames {
    pub fn new(gpu: &VulkanRenderer) -> Result<Self, Error> {
        Ok(Self {
            frames: StreamFrames::default(),
            latched: BTreeMap::new(),
            imports: BTreeMap::new(),
            importer: gpu.hardware_buffer_importer().map_err(Error::Importer)?,
            release: ReleaseAfter::Fence(None),
            held: Vec::new(),
        })
    }

    pub fn frames(&self) -> &StreamFrames {
        &self.frames
    }

    #[cfg(target_os = "android")]
    pub fn format_properties(
        &self,
        buffer: &crate::platform::ndk::hardware_buffer::HardwareBuffer,
    ) -> Result<lavashell_renderer::vulkan::FormatProperties, Error> {
        (self.importer.format_properties(buffer)).map_err(Error::BufferFormat)
    }

    pub fn latch(&mut self, painter: &Painter, stream: &mut SurfaceStream) -> Result<Latch, Error> {
        let mut failures = Failures::default();
        let removed: BTreeSet<_> = (stream.removed())
            .filter_map(|removal| match removal {
                Ok(id) => Some(id),
                Err(failure) => {
                    failures.push(failure);
                    None
                }
            })
            .collect();
        self.imports.retain(|id, _| !removed.contains(id));
        failures.into_result()?;
        self.return_held()?;
        let frame = match stream.take()? {
            Acquired::Nothing => return Ok(Latch::Unchanged),
            Acquired::Rejected(rejection) => {
                painter.reject_stream_frame(rejection);
                return Ok(Latch::Rejected(rejection));
            }
            Acquired::Frame(frame) => frame,
        };
        let sampling = self.importer.properties(&frame.buffer);
        let properties = match sampling.map_err(Error::BufferProperties)? {
            Sampling::Sampled(properties) => properties,
            Sampling::Rejected(rejection) => {
                frame.release(None);
                painter.reject_stream_frame(rejection);
                return Ok(Latch::Rejected(rejection));
            }
        };
        // The frame latched before returns to its reader now, because `record_latch` below can delete that reader.
        self.unlatch(stream.id())?;
        let Entry::Vacant(slot) = self.latched.entry(stream.id()) else {
            frame.release(None);
            return Err(Error::StillLatched);
        };
        let latched = slot.insert(Latched {
            image: stream.record_latch(&frame),
            frame,
            properties,
            import: None,
        });
        Ok(Latch::Frame(latched.image))
    }

    /// Imports the latched frames drawn by `tree` and not drawn by a paint before. A frame that
    /// fails to import is released and recorded as a stream rejection with the cause. That stream
    /// draws nothing. Even when the release of such a frame fails, the frames of the other streams
    /// are imported.
    pub fn import(
        &mut self,
        gpu: &mut VulkanRenderer,
        painter: &Painter,
        tree: Presented<'_>,
    ) -> Result<(), Error> {
        self.prune();
        if self
            .latched
            .values()
            .all(|latched| latched.import.is_some())
        {
            return Ok(());
        }
        let drawn: BTreeSet<_> = (tree.leaf_commands())
            .filter_map(|command| match command {
                DrawCommand::StreamImage { image, .. } => Some(image.stream),
                _ => None,
            })
            .collect();
        let mut failures = Failures::default();
        for stream in drawn {
            let pending = self
                .latched
                .get(&stream)
                .is_some_and(|latched| latched.import.is_none());
            if !pending {
                continue;
            }
            if let Err(cause) = self.bind(gpu, stream) {
                failures.add(self.unlatch(stream));
                painter.reject_stream_import(&cause);
            }
        }
        failures.into_result()
    }

    fn bind(&mut self, gpu: &mut VulkanRenderer, stream: StreamId) -> Result<(), Error> {
        let importer = &self.importer;
        let Some(latched) = self.latched.get_mut(&stream) else {
            return Ok(());
        };
        let frame = &latched.frame;
        let import = match self.imports.entry(frame.id) {
            Entry::Occupied(cached) => cached.get().import.clone(),
            Entry::Vacant(slot) => {
                let import = importer
                    .import(&latched.properties)
                    .map_err(Error::ImportBuffer)?;
                let cached = CachedImport {
                    reader: frame.reader.clone(),
                    import: Rc::new(import),
                };
                slot.insert(cached).import.clone()
            }
        };
        let pixels = importer.acquire(gpu, &import).map_err(Error::WrapImport)?;
        self.frames.bind(&latched.image, pixels, frame.origin);
        latched.import = Some(import);
        Ok(())
    }

    /// Exports the release fence after a render that could draw an imported frame. Then returns the
    /// held frames whose work has finished and destroys the imports that the GPU no longer uses.
    pub fn release(&mut self, gpu: &mut VulkanRenderer) -> Result<(), Error> {
        let imports = (self.latched.values())
            .filter_map(|latched| latched.import.clone())
            .collect::<Vec<_>>();
        let mut failures = Failures::default();
        if !imports.is_empty() {
            let released = (self.importer.release(gpu, imports.iter().map(Rc::as_ref)))
                .map_err(Error::HandBack);
            let after = match released {
                Ok(fence) => ReleaseAfter::Fence(fence),
                Err(error) => {
                    failures.push(error);
                    ReleaseAfter::Work(self.submitted_work(&mut failures))
                }
            };
            if let ReleaseAfter::Work(Work::Submitted(work)) =
                std::mem::replace(&mut self.release, after)
            {
                self.hold(None, Work::Submitted(work));
            }
        }
        failures.add(self.return_held());
        failures.add(self.importer.poll().map_err(Error::RetireImports));
        failures.into_result()
    }

    /// Removes the frame latched from `stream`. The buffer of that frame returns to the producer once the GPU has read
    /// the buffer for the last time.
    fn unlatch(&mut self, stream: StreamId) -> Result<(), Error> {
        let Some(latched) = self.latched.remove(&stream) else {
            return Ok(());
        };
        self.frames.retire(stream);
        if latched.import.is_none() {
            latched.frame.release(None);
            return Ok(());
        }
        let copied = match &self.release {
            ReleaseAfter::Fence(fence) => fence.as_ref().map(OwnedFd::try_clone).transpose(),
            ReleaseAfter::Work(work) => {
                let work = work.clone();
                self.hold(Some(latched.frame), work);
                return Ok(());
            }
        };
        match copied {
            Ok(fence) => {
                latched.frame.release(fence);
                Ok(())
            }
            Err(error) => {
                // Without a copy of the fence, the frame is held until `return_held` finds the GPU work finished.
                let mut failures = Failures::default();
                failures.push(Error::CopyReleaseFence(error));
                let work = self.submitted_work(&mut failures);
                self.hold(Some(latched.frame), work);
                failures.into_result()
            }
        }
    }

    /// The completion of the GPU work submitted so far. When the device cannot submit a fence for
    /// that work, the failure is added to `failures` and the work is `Work::Unknown`.
    fn submitted_work(&mut self, failures: &mut Failures) -> Work {
        let completion = self.importer.completion().map_err(Error::SubmitCompletion);
        match completion {
            Ok(completion) => Work::Submitted(Rc::new(completion)),
            Err(error) => {
                failures.push(error);
                Work::Unknown
            }
        }
    }

    fn hold(&mut self, frame: Option<QueuedFrame>, work: Work) {
        self.held.push(Held {
            work,
            _frame: frame,
        });
    }

    /// Returns the held frames whose work the GPU has finished to their producers. Frames held for
    /// `Work::Unknown` are returned only at teardown.
    fn return_held(&mut self) -> Result<(), Error> {
        let mut failures = Failures::default();
        self.held.retain(|held| match &held.work {
            Work::Submitted(completion) => match completion.status() {
                Ok(status) => status == Status::Pending,
                Err(error) => {
                    failures.push(Error::CompletionStatus(error));
                    true
                }
            },
            Work::Unknown => true,
        });
        failures.into_result()
    }

    fn prune(&mut self) {
        self.imports.retain(|_, cached| cached.reader.is_alive());
        self.latched.retain(|stream, latched| {
            let alive = latched.frame.reader.is_alive();
            if !alive {
                self.frames.retire(*stream);
            }
            alive
        });
    }
}
