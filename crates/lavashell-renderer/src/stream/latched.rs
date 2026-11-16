// SPDX-License-Identifier: GPL-3.0-only
use super::release::ReleaseTracker;
use super::{
    BufferQueue, FrameDevice, FrameSource, ImportHandBack, Latch, LatchAccess, ProducedFrame,
    Stream, StreamBackend, Taken,
};
use crate::error::FailureList;
use crate::{Painter, Sampling, StreamFrames};
use lavashell_core::prelude::{DrawCommand, Presented, StreamId, StreamImage};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

struct Latched<I: ImportHandBack> {
    image: StreamImage,
    frame: I::Frame,
    sampled: I::Sampled,
    import: Option<Rc<I::Import>>,
}

impl<I: ImportHandBack> Latched<I> {
    fn release(
        self,
        releases: &mut ReleaseTracker<I, I::Frame>,
        importer: &I,
    ) -> Result<(), I::Error> {
        if self.import.is_none() {
            self.frame.release(None);
            return Ok(());
        }
        releases.release(importer, self.frame)
    }
}

struct CachedImport<I: ImportHandBack> {
    queue: <I::Frame as ProducedFrame>::Queue,
    import: Rc<I::Import>,
}

pub struct LatchedFrames<I: ImportHandBack> {
    frames: StreamFrames,
    latched: BTreeMap<StreamId, Latched<I>>,
    imports: BTreeMap<<I::Frame as ProducedFrame>::BufferKey, CachedImport<I>>,
    importer: I,
    releases: ReleaseTracker<I, I::Frame>,
}

impl<I: ImportHandBack> LatchedFrames<I> {
    #[must_use]
    pub fn new(importer: I) -> Self {
        Self {
            frames: StreamFrames::default(),
            latched: BTreeMap::new(),
            imports: BTreeMap::new(),
            importer,
            releases: ReleaseTracker::default(),
        }
    }

    #[must_use]
    pub fn importer(&self) -> &I {
        &self.importer
    }

    pub fn latch<S>(&mut self, painter: &Painter, stream: &mut Stream<S>) -> Result<Latch, I::Error>
    where
        S: FrameSource<Frame = I::Frame>,
        I::Error: From<S::Error>,
    {
        let access = LatchAccess(());
        let mut failures = FailureList::default();
        let removed: BTreeSet<_> = (stream.source.take_removed_buffers(&access))
            .filter_map(|removal| match removal {
                Ok(buffer) => Some(buffer),
                Err(failure) => {
                    failures.push(I::Error::from(failure));
                    None
                }
            })
            .collect();
        self.imports.retain(|buffer, _| !removed.contains(buffer));
        failures.into_result()?;
        self.releases.return_finished(&self.importer)?;
        let frame = match stream.source.take_newest(&access)? {
            Taken::Nothing => return Ok(Latch::Unchanged),
            Taken::Rejected(rejection) => {
                painter.reject_stream_frame(rejection);
                return Ok(Latch::Rejected(rejection));
            }
            Taken::Frame(frame) => frame,
        };
        let sampled = match self.importer.sample(&frame)? {
            Sampling::Sampled(sampled) => sampled,
            Sampling::Rejected(rejection) => {
                frame.release(None);
                painter.reject_stream_frame(rejection);
                return Ok(Latch::Rejected(rejection));
            }
        };
        let image = stream.next_image(&frame);
        let latched = Latched {
            image,
            frame,
            sampled,
            import: None,
        };
        let (latched, released) = match self.latched.entry(stream.id) {
            Entry::Vacant(slot) => (slot.insert(latched), Ok(())),
            Entry::Occupied(mut slot) => {
                let previous = slot.insert(latched);
                self.frames.retire(stream.id);
                let released = previous.release(&mut self.releases, &self.importer);
                (slot.into_mut(), released)
            }
        };
        stream.source.record_latch(&access, &latched.frame);
        released?;
        Ok(Latch::Frame(image))
    }

    fn import(
        &mut self,
        gpu: &mut I::Gpu,
        painter: &Painter,
        tree: Presented<'_>,
    ) -> Result<(), I::Error> {
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
        let mut failures = FailureList::default();
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

    fn bind(&mut self, gpu: &mut I::Gpu, stream: StreamId) -> Result<(), I::Error> {
        let importer = &self.importer;
        let Some(latched) = self.latched.get_mut(&stream) else {
            return Ok(());
        };
        let frame = &latched.frame;
        let import = match self.imports.entry(frame.buffer_key()) {
            Entry::Occupied(cached) => cached.get().import.clone(),
            Entry::Vacant(slot) => {
                let cached = CachedImport {
                    queue: frame.queue().clone(),
                    import: Rc::new(importer.import(&latched.sampled)?),
                };
                slot.insert(cached).import.clone()
            }
        };
        let pixels = importer.acquire(gpu, &import)?;
        self.frames.bind(&latched.image, pixels, frame.origin());
        latched.import = Some(import);
        Ok(())
    }

    fn release(&mut self, gpu: &mut I::Gpu) -> Result<(), I::Error> {
        let imports = (self.latched.values())
            .filter_map(|latched| latched.import.clone())
            .collect::<Vec<_>>();
        let mut failures = FailureList::default();
        if !imports.is_empty() {
            let handed_back = self.importer.release(gpu, &imports);
            failures.add(self.releases.wait_for(&self.importer, handed_back));
        }
        failures.add(self.releases.return_finished(&self.importer));
        failures.add(self.importer.poll());
        failures.into_result()
    }

    fn unlatch(&mut self, stream: StreamId) -> Result<(), I::Error> {
        let Some(latched) = self.latched.remove(&stream) else {
            return Ok(());
        };
        self.frames.retire(stream);
        latched.release(&mut self.releases, &self.importer)
    }

    fn prune(&mut self) {
        self.imports.retain(|_, cached| cached.queue.exists());
        self.latched.retain(|stream, latched| {
            let exists = latched.frame.queue().exists();
            if !exists {
                self.frames.retire(*stream);
            }
            exists
        });
    }
}

impl<I: ImportHandBack> StreamBackend for LatchedFrames<I> {
    type Gpu = I::Gpu;
    type Error = I::Error;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        tree: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), I::Error>) {
        let mut failures = FailureList::default();
        failures.add(self.import(gpu, painter, tree));
        let drawn = draw(FrameDevice {
            gpu,
            painter,
            streams: &self.frames,
        });
        failures.add(self.release(gpu));
        (drawn, failures.into_result())
    }
}
