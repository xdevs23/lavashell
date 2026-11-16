// SPDX-License-Identifier: GPL-3.0-only
use super::reader::{MEDIA, Media, QUEUES};
use super::{GPU, Gpu};
use std::rc::Rc;

pub(crate) struct Simulation {
    pub gpu: Rc<Gpu>,
    pub media: Rc<Media>,
}

impl Simulation {
    pub(crate) fn start() -> Self {
        let (gpu, media) = (Rc::<Gpu>::default(), Rc::<Media>::default());
        drop(GPU.replace(gpu.clone()));
        drop(MEDIA.replace(media.clone()));
        drop(QUEUES.take());
        Self { gpu, media }
    }
}

impl Drop for Simulation {
    fn drop(&mut self) {
        drop(GPU.take());
        drop(MEDIA.take());
        drop(QUEUES.take());
    }
}
