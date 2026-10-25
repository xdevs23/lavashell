// SPDX-License-Identifier: GPL-3.0-only
//! The release of a scene image to the external queue family. The image is exported once the queue has completed the
//! release.

use super::{ExternalImage, RELEASED};
use crate::Error;
use lavashell_vulkan::{Commands, ExportableImage, Recorded, Recording, Status, Submitted};
use std::any::Any;
use std::sync::Arc;

/// The release of a scene image to the external queue family, submitted to the queue.
pub struct PendingRelease {
    submitted: Submitted,
    image: ExportableImage,
    lease: Arc<dyn Any + Send + Sync>,
}

/// A release that the queue has completed: the exported image and the commands that recorded the release.
pub struct CompletedRelease {
    pub image: ExternalImage,
    pub commands: Commands,
}

pub enum ReleaseProgress {
    Pending(PendingRelease),
    Complete(CompletedRelease),
}

impl PendingRelease {
    /// Appends the release of `image` to the external queue family to `recording`, which writes the image, and
    /// submits the recording without waiting for it. An importer can use its import of the image as long as the
    /// producer has `image`. The importer drops `lease` once it no longer reads the image.
    ///
    /// # Errors
    /// Fails when the recording does not know the state of the image, or Vulkan cannot submit.
    pub fn submit(
        mut recording: Recording,
        image: &ExportableImage,
        lease: Arc<dyn Any + Send + Sync>,
    ) -> Result<Self, Error> {
        recording
            .transition(image.image(), RELEASED)
            .map_err(Error::ExportScene)?;
        let submitted = recording
            .finish()
            .and_then(Recorded::submit)
            .map_err(Error::ExportScene)?;
        Ok(Self {
            submitted,
            image: image.clone(),
            lease,
        })
    }

    /// Checks without waiting whether the queue has completed the release. Returns the pending release while the
    /// queue has not completed it, and the exported image once it has.
    ///
    /// # Errors
    /// Fails when the device is lost or the image cannot be exported.
    pub fn poll(self) -> Result<ReleaseProgress, Error> {
        match self.submitted.status().map_err(Error::ExportScene)? {
            Status::Pending => Ok(ReleaseProgress::Pending(self)),
            Status::Complete => self.wait().map(ReleaseProgress::Complete),
        }
    }

    /// Waits until the queue has completed the release, and exports the image.
    ///
    /// # Errors
    /// Fails when the device is lost or the image cannot be exported.
    pub fn wait(self) -> Result<CompletedRelease, Error> {
        let commands = self.submitted.reclaim().map_err(Error::ExportScene)?;
        let owner = Arc::downgrade(self.image.image());
        let image = ExternalImage {
            export: self.image.export().map_err(Error::ExportScene)?,
            owner,
            lease: self.lease,
        };
        Ok(CompletedRelease { image, commands })
    }
}
