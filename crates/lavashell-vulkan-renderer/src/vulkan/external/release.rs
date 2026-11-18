// SPDX-License-Identifier: GPL-3.0-only

use super::{ExternalImage, RELEASED, SceneLease};
use crate::Error;
use lavashell_vulkan::{Commands, ExportableImage, Recorded, Recording, Status, Submitted};
use std::sync::Arc;

pub struct PendingRelease {
    submitted: Submitted,
    image: ExportableImage,
    lease: Arc<dyn SceneLease>,
}

pub struct CompletedRelease {
    pub image: ExternalImage,
    pub commands: Commands,
}

pub enum ReleaseProgress {
    Pending(PendingRelease),
    Complete(CompletedRelease),
}

impl PendingRelease {
    pub fn submit(
        mut recording: Recording,
        image: &ExportableImage,
        lease: Arc<dyn SceneLease>,
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

    pub fn poll(self) -> Result<ReleaseProgress, Error> {
        match self.submitted.status().map_err(Error::ExportScene)? {
            Status::Pending => Ok(ReleaseProgress::Pending(self)),
            Status::Complete => self.wait().map(ReleaseProgress::Complete),
        }
    }

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
