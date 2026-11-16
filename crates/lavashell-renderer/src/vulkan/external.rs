// SPDX-License-Identifier: GPL-3.0-only

mod allocation;
mod release;
#[cfg(test)]
mod tests;

use super::completion::finish_completed;
use super::deferred_release::{DeferredRelease, ReleaseQueue};
use super::skia_handoff::{set_texture_states, submit_recorded};
use super::{
    CompletedScene, Completion, PendingScene, SceneCompletion, SceneImage, VulkanRenderer,
};
use crate::Error;
use crate::error::FailureList;
use allocation::{Allocation, RELEASED};
use lavashell_vulkan::{ExportableImage, Image, OpaqueFdExport, Recording, Status, Submitted};
pub use release::{CompletedRelease, PendingRelease, ReleaseProgress};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{AlphaType, ColorSpace, ColorType};
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub trait SceneLease: Send + Sync {}

pub struct ExternalImage {
    export: OpaqueFdExport,
    owner: Weak<Image>,
    lease: Arc<dyn SceneLease>,
}

impl ExternalImage {
    pub fn release(
        recording: Recording,
        image: &ExportableImage,
        lease: Arc<dyn SceneLease>,
    ) -> Result<Self, Error> {
        let completed = PendingRelease::submit(recording, image, lease)?.wait()?;
        Ok(completed.image)
    }
}

#[derive(Default)]
pub(super) struct Imports {
    allocations: Vec<Imported>,
    queued: ReleaseQueue<Leased>,
    released: Vec<Released>,
    evicted: Vec<Evicted>,
    kept_until_idle: Vec<KeptUntilIdle>,
}

struct Imported {
    owner: Weak<Image>,
    allocation: Rc<Allocation>,
}

pub(super) struct Leased {
    allocation: Rc<Allocation>,
    source: Arc<dyn SceneLease>,
    acquired: Submitted,
}

struct Released {
    allocation: Rc<Allocation>,
    acquired: Submitted,
    handed_back: Submitted,
    source: Arc<dyn SceneLease>,
}

impl Released {
    fn status(&self) -> Result<Status, lavashell_vulkan::Error> {
        Ok(
            match (self.acquired.status()?, self.handed_back.status()?) {
                (Status::Complete, Status::Complete) => Status::Complete,
                _ => Status::Pending,
            },
        )
    }

    fn reclaim(self) -> Result<(), Error> {
        let acquired = self.acquired.reclaim().map_err(Error::ReleaseSceneStatus)?;
        let handed_back = self
            .handed_back
            .reclaim()
            .map_err(Error::ReleaseSceneStatus)?;
        self.allocation.returned([acquired, handed_back]);
        drop(self.source);
        Ok(())
    }
}

struct Evicted {
    completion: Completion,
    allocations: Vec<Rc<Allocation>>,
}

impl Evicted {
    fn finish(self) {
        drop(self.allocations);
    }
}

enum KeptUntilIdle {
    FailedHandBack(Leased),
    FailedEviction(Rc<Allocation>),
}

impl KeptUntilIdle {
    fn release(self) {
        match self {
            Self::FailedHandBack(leased) => drop(leased),
            Self::FailedEviction(allocation) => drop(allocation),
        }
    }
}

impl Imports {
    pub fn clear(&mut self) {
        self.released.clear();
        self.evicted.clear();
        self.kept_until_idle
            .drain(..)
            .for_each(KeptUntilIdle::release);
        self.allocations.clear();
    }

    fn drop_finished(&mut self) -> Result<(), Error> {
        let mut failures = FailureList::default();
        failures.add(finish_completed(
            &mut self.evicted,
            |evicted| &evicted.completion,
            Evicted::finish,
        ));
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
    hand_back: DeferredRelease<Leased>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(leased) = self.leased.take() {
            let _dropped_after_the_renderer = self.hand_back.defer(leased);
        }
    }
}

impl VulkanRenderer {
    pub fn poll_scene_imports(&mut self) -> Result<(), Error> {
        let queued = self.imported.queued.take_deferred();
        let mut failures = FailureList::default();
        failures.add(self.hand_back(queued));
        failures.add(self.imported.drop_finished());
        failures.add(self.evict_dropped());
        failures.into_result()
    }

    fn hand_back(&mut self, queued: Vec<Leased>) -> Result<(), Error> {
        if queued.is_empty() {
            return Ok(());
        }
        let general = skvk::mutable_texture_states::new_vulkan(
            skvk::ImageLayout::GENERAL,
            self.context.device().queue_family(),
        );
        let textures = queued.iter().map(|leased| &leased.allocation.texture);
        let moved =
            set_texture_states(&mut self.context, textures, &general, Error::HandBackScenes);
        let imports = &mut self.imported;
        if let Err(error) = moved {
            let failed = queued.into_iter().map(KeptUntilIdle::FailedHandBack);
            imports.kept_until_idle.extend(failed);
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
                    source: leased.source,
                }),
                Err(error) => {
                    (imports.kept_until_idle).push(KeptUntilIdle::FailedHandBack(leased));
                    failures.push(error);
                }
            }
        }
        failures.into_result()
    }

    fn evict_dropped(&mut self) -> Result<(), Error> {
        let imports = &mut self.imported;
        let (dropped, kept) = std::mem::take(&mut imports.allocations)
            .into_iter()
            .partition::<Vec<_>, _>(|imported| {
                imported.owner.strong_count() == 0 && Rc::strong_count(&imported.allocation) == 1
            });
        imports.allocations = kept;
        if dropped.is_empty() {
            return Ok(());
        }
        let purged =
            (self.context).purge_unlocked_resources(gpu::PurgeResourceOptions::AllResources);
        let allocations = dropped.into_iter().map(|imported| imported.allocation);
        match submit_recorded(purged)
            .and_then(|()| Completion::after_submitted(self.context.device()))
        {
            Ok(completion) => {
                imports.evicted.push(Evicted {
                    completion,
                    allocations: allocations.collect(),
                });
                Ok(())
            }
            Err(error) => {
                let failed = allocations.map(KeptUntilIdle::FailedEviction);
                imports.kept_until_idle.extend(failed);
                Err(error)
            }
        }
    }

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
        // queue family, with the release complete, because only `PendingRelease` makes one after it records that
        // release and waits for it. The import consumes the `ExternalImage`, so each release is acquired
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
            hand_back: self.imported.queued.create_handle(),
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
        let imports = &mut self.imported;
        let existing = (imports.allocations.iter())
            .find(|imported| Weak::ptr_eq(&imported.owner, &external.owner));
        if let Some(imported) = existing {
            return Ok(imported.allocation.clone());
        }
        let allocation = Rc::new(Allocation::import(self.context.device(), &external.export)?);
        imports.allocations.push(Imported {
            owner: external.owner.clone(),
            allocation: allocation.clone(),
        });
        Ok(allocation)
    }
}
