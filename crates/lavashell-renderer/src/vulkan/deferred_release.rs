// SPDX-License-Identifier: GPL-3.0-only
use std::sync::mpsc::{Receiver, Sender, channel};

pub(super) struct ReleaseQueue<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> Default for ReleaseQueue<T> {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Self { sender, receiver }
    }
}

impl<T> ReleaseQueue<T> {
    pub fn create_handle(&self) -> DeferredRelease<T> {
        DeferredRelease(self.sender.clone())
    }

    pub fn take_deferred(&self) -> Vec<T> {
        self.receiver.try_iter().collect()
    }
}

pub(super) struct DeferredRelease<T>(Sender<T>);

impl<T> DeferredRelease<T> {
    pub fn defer(&self, value: T) -> Result<(), T> {
        self.0.send(value).map_err(|unreceived| unreceived.0)
    }
}
