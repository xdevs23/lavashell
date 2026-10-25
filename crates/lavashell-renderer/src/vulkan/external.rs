// SPDX-License-Identifier: GPL-3.0-only
//! The import of scene images exported by another Vulkan device. The renderer samples an imported image directly,
//! without a pixel copy. The producer does not write the image while a scene of this renderer reads it.

mod allocation;
mod release;
#[cfg(test)]
mod tests;

use super::skia_handoff::submit_recorded;
use super::{
    CompletedScene, Completion, PendingScene, SceneCompletion, SceneImage, VulkanRenderer,
};
use crate::Error;
use crate::error::FailureList;
use allocation::{Allocation, RELEASED};
use lavashell_vulkan::{ExportableImage, OpaqueFdExport, Recording, Status, Submitted};
pub use release::{CompletedRelease, PendingRelease, ReleaseProgress};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{AlphaType, ColorSpace, ColorType};
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::time::Duration;

/// A scene image released by the producer to the external queue family, ready for an import without a pixel copy.
pub struct ExternalImage {
    export: OpaqueFdExport,
    owner: Weak<dyn Any + Send + Sync>,
    lease: Arc<dyn Any + Send + Sync>,
}

impl ExternalImage {
    /// A scene image released by a producer outside this crate. An importer reuses its import of the image in
    /// `export` as long as `owner` is alive. The importer drops the import once `owner` is gone and no scene reads
    /// the image anymore.
    ///
    /// # Safety
    /// The producer must have released the image of `export` from its queue family to `VK_QUEUE_FAMILY_EXTERNAL`
    /// in the `GENERAL` layout on both sides of the barrier, and that release must be complete. The producer must not
    /// write the image again until `lease` is dropped. `owner` must be alive as long as the producer has the
    /// allocation of `export`, and no other allocation may share the same `owner`.
    #[must_use]
    pub unsafe fn from_foreign_export(
        export: OpaqueFdExport,
        owner: Weak<dyn Any + Send + Sync>,
        lease: Arc<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            export,
            owner,
            lease,
        }
    }

    /// Appends the release of `image` to the external queue family to `recording`, which writes the image, waits
    /// until the release is complete and exports the image. An importer can use its import of the image as long as
    /// the producer has `image`. The importer drops `lease` once it no longer reads the image.
    ///
    /// # Errors
    /// Fails when the recording does not know the state of the image, or Vulkan cannot submit or export.
    pub fn release(
        recording: Recording,
        image: &ExportableImage,
        lease: Arc<dyn Any + Send + Sync>,
    ) -> Result<Self, Error> {
        let completed = PendingRelease::submit(recording, image, lease)?.wait()?;
        Ok(completed.image)
    }
}

/// The images imported by this renderer from producers. Once no scene reads an image anymore, the image is handed
/// back: the queue of this renderer releases the image back to the producer.
#[derive(Default)]
pub(super) struct Imports {
    allocations: Vec<Imported>,
    queued: Vec<Leased>,
    released: Vec<Released>,
    evicted: Vec<Evicted>,
    stranded: Vec<Stranded>,
}

struct Imported {
    owner: Weak<dyn Any + Send + Sync>,
    allocation: Rc<Allocation>,
}

/// An imported image that a scene reads, with the submission that acquired the image from the producer.
pub(super) struct Leased {
    allocation: Rc<Allocation>,
    source: Arc<dyn Any + Send + Sync>,
    acquired: Submitted,
}

/// An imported image that is being handed back to the producer, with the submission that acquired the image and the
/// submission that hands it back.
struct Released {
    allocation: Rc<Allocation>,
    acquired: Submitted,
    handed_back: Submitted,
    _source: Arc<dyn Any + Send + Sync>,
}

impl Released {
    /// Whether the acquire and the hand-back are both complete, checked without waiting.
    fn status(&self) -> Result<Status, lavashell_vulkan::Error> {
        Ok(
            match (self.acquired.status()?, self.handed_back.status()?) {
                (Status::Complete, Status::Complete) => Status::Complete,
                _ => Status::Pending,
            },
        )
    }

    /// Marks the image as being with the producer again, and stores the command buffers of the completed acquire and
    /// hand-back in the allocation for reuse.
    fn reclaim(self) -> Result<(), Error> {
        let acquired = self.acquired.reclaim().map_err(Error::ReleaseSceneStatus)?;
        let handed_back = self
            .handed_back
            .reclaim()
            .map_err(Error::ReleaseSceneStatus)?;
        self.allocation.returned([acquired, handed_back]);
        Ok(())
    }
}

/// Imported images dropped by their producer. They are freed once the GPU has finished the work submitted before
/// their eviction.
struct Evicted {
    completion: Completion,
    _allocations: Vec<Rc<Allocation>>,
}

/// An imported image that could not be handed back to the producer after a failure of the device. It is freed only
/// when the renderer is dropped.
struct Stranded {
    _allocation: Rc<Allocation>,
    _source: Option<Arc<dyn Any + Send + Sync>>,
    _acquired: Option<Submitted>,
}

impl Imports {
    pub fn clear(&mut self) {
        self.released.clear();
        self.queued.clear();
        self.evicted.clear();
        self.stranded.clear();
        self.allocations.clear();
    }

    fn strand(&mut self, leased: Leased) {
        self.stranded.push(Stranded {
            _allocation: leased.allocation,
            _source: Some(leased.source),
            _acquired: Some(leased.acquired),
        });
    }

    fn strand_dropped(&mut self, allocation: Rc<Allocation>) {
        self.stranded.push(Stranded {
            _allocation: allocation,
            _source: None,
            _acquired: None,
        });
    }

    /// Frees the evicted images and gives the handed-back images back to their producers once their GPU work is
    /// complete, without waiting for the pending work.
    fn drop_finished(&mut self) -> Result<(), Error> {
        let mut failures = FailureList::default();
        failures.add(super::drop_completed(&mut self.evicted, |evicted| {
            &evicted.completion
        }));
        for released in std::mem::take(&mut self.released) {
            match released.status() {
                Ok(Status::Pending) => self.released.push(released),
                Ok(Status::Complete) => failures.add(released.reclaim()),
                Err(error) => {
                    self.released.push(released);
                    failures.push(Error::ReleaseSceneStatus(error));
                }
            }
        }
        failures.into_result()
    }
}

pub(super) struct Lease {
    leased: Option<Leased>,
    imports: Rc<RefCell<Imports>>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        // The next poll of the imports flushes the Skia reads of the image, then submits the hand-back.
        if let Some(leased) = self.leased.take() {
            self.imports.borrow_mut().queued.push(leased);
        }
    }
}

impl VulkanRenderer {
    /// Hands the images of dropped scenes back to their producers, and evicts the images dropped by their producers,
    /// without waiting for the GPU. An imported image is freed only after its producer dropped it. A producer that
    /// reuses the image for a later scene therefore gets the same import, and the references to the image in the
    /// Skia cache are still valid.
    ///
    /// # Errors
    /// Fails when Skia or Vulkan cannot hand an image back or cannot read the status of GPU work. Both happen after
    /// the loss of the device.
    pub fn poll_scene_imports(&mut self) -> Result<(), Error> {
        let queued = std::mem::take(&mut self.imported.borrow_mut().queued);
        let mut failures = FailureList::default();
        failures.add(self.hand_back(queued));
        failures.add(self.imported.borrow_mut().drop_finished());
        failures.add(self.evict_dropped());
        failures.into_result()
    }

    fn hand_back(&mut self, queued: Vec<Leased>) -> Result<(), Error> {
        if queued.is_empty() {
            return Ok(());
        }
        let general = skvk::mutable_texture_states::new_vulkan(
            skvk::ImageLayout::GENERAL,
            self.device.queue_family(),
        );
        let context = &mut *self.context;
        let mut refused = false;
        for leased in &queued {
            refused |= !context.set_backend_texture_state(&leased.allocation.texture, &general);
        }
        // The Skia reads of the images and their move into GENERAL are submitted before the hand-back.
        let refusal = if refused {
            Err(Error::HandBackScenes)
        } else {
            Ok(())
        };
        let moved = submit_recorded(context).and(refusal);
        let mut imports = self.imported.borrow_mut();
        if let Err(error) = moved {
            for leased in queued {
                imports.strand(leased);
            }
            return Err(error);
        }
        let mut failures = FailureList::default();
        for leased in queued {
            // SAFETY: the hand-back needs the image in GENERAL on this queue, and only Skia tracks that layout.
            // `set_backend_texture_state` succeeded for every queued image, and the flush above submitted the move
            // into GENERAL before this submission.
            match unsafe { leased.allocation.release() } {
                Ok(handed_back) => imports.released.push(Released {
                    allocation: leased.allocation,
                    acquired: leased.acquired,
                    handed_back,
                    _source: leased.source,
                }),
                Err(error) => {
                    imports.strand(leased);
                    failures.push(error);
                }
            }
        }
        failures.into_result()
    }

    /// Evicts the images that their producers dropped and that no scene reads. They are freed once the GPU work
    /// submitted before the eviction is complete.
    fn evict_dropped(&mut self) -> Result<(), Error> {
        let mut imports = self.imported.borrow_mut();
        let (dropped, kept) = std::mem::take(&mut imports.allocations)
            .into_iter()
            .partition::<Vec<_>, _>(|imported| {
                imported.owner.strong_count() == 0 && Rc::strong_count(&imported.allocation) == 1
            });
        imports.allocations = kept;
        if dropped.is_empty() {
            return Ok(());
        }
        // After the purge, the Skia cache contains no texture of the dropped images. The last Skia work on those
        // images is submitted before the completion that frees them.
        let purged =
            (self.context).purge_unlocked_resources(gpu::PurgeResourceOptions::AllResources);
        let allocations = dropped.into_iter().map(|imported| imported.allocation);
        match submit_recorded(purged).and_then(|()| Completion::after_submitted(&self.device)) {
            Ok(completion) => {
                imports.evicted.push(Evicted {
                    completion,
                    _allocations: allocations.collect(),
                });
                Ok(())
            }
            Err(error) => {
                for allocation in allocations {
                    imports.strand_dropped(allocation);
                }
                Err(error)
            }
        }
    }

    /// Imports a scene image rendered by another Vulkan device, and acquires the image from the external queue family
    /// on the queue of this renderer. The import makes no pixel copy and does not wait on the CPU.
    ///
    /// # Errors
    /// Fails when the image is not in sRGB RGBA, belongs to another GPU, is still read by a scene of an earlier
    /// import, or Vulkan or Skia cannot import it.
    pub fn import_scene(
        &mut self,
        external: ExternalImage,
        generation: u64,
        produced_at: Duration,
    ) -> Result<PendingScene, Error> {
        self.poll_scene_imports()?;
        let allocation = self.imported_allocation(&external)?;
        // SAFETY: recording the acquire needs the state in which the producer left the image, and only the producer
        // can guarantee that state. An `ExternalImage` exists only for an image released in GENERAL to the external
        // queue family, with the release complete. `PendingRelease` records that release, and the safety contract of
        // `from_foreign_export` requires it. The import consumes the `ExternalImage`, so each release is acquired
        // once. The allocation returns an error for an acquire while the renderer still has the image.
        let acquired = unsafe { allocation.acquire() }?;
        let image = gpu::images::borrow_texture_from(
            &mut self.context,
            &allocation.texture,
            SurfaceOrigin::TopLeft,
            ColorType::SRGBA8888,
            AlphaType::Opaque,
            ColorSpace::new_srgb_linear(),
        );
        let lease = Lease {
            leased: Some(Leased {
                allocation,
                source: external.lease,
                acquired,
            }),
            imports: self.imported.clone(),
        };
        let image = image.ok_or(Error::WrapScene)?;
        Ok(PendingScene {
            scene: CompletedScene {
                generation,
                produced_at,
                content: SceneImage::new(&self.context, image, Some(lease)),
            },
            completion: SceneCompletion::already_written(),
        })
    }

    fn imported_allocation(&mut self, external: &ExternalImage) -> Result<Rc<Allocation>, Error> {
        let mut imports = self.imported.borrow_mut();
        let existing = (imports.allocations.iter())
            .find(|imported| Weak::ptr_eq(&imported.owner, &external.owner));
        if let Some(imported) = existing {
            return Ok(imported.allocation.clone());
        }
        let allocation = Rc::new(Allocation::import(&self.device, &external.export)?);
        imports.allocations.push(Imported {
            owner: external.owner.clone(),
            allocation: allocation.clone(),
        });
        Ok(allocation)
    }
}
