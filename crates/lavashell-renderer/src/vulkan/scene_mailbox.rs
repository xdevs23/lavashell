// SPDX-License-Identifier: GPL-3.0-only
//! A mailbox for scene frames whose GPU work may still be pending. The mailbox returns a scene as the latest one only
//! once its GPU work is complete. It contains at most a set number of pending scenes. Polling the mailbox never waits
//! for their GPU work.

use crate::Error;
use lavashell_vulkan::Status;
use std::{collections::VecDeque, sync::Arc, time::Duration};

pub struct CompletedScene<T> {
    pub generation: u64,
    pub produced_at: Duration,
    pub content: T,
}

struct Pending<T, C> {
    scene: CompletedScene<T>,
    completion: C,
}

pub struct SceneMailbox<T, C> {
    pending: VecDeque<Pending<T, C>>,
    latest: Option<Arc<CompletedScene<T>>>,
    capacity: usize,
    last_published: u64,
}

impl<T, C> SceneMailbox<T, C> {
    /// A mailbox that contains at most `capacity` scenes whose GPU work is pending.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            latest: None,
            capacity,
            last_published: 0,
        }
    }

    /// Changes the largest number of scenes whose GPU work may be pending. A smaller capacity drops no pending scene.
    /// Each of them is removed once its GPU work is complete.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    #[must_use]
    pub fn can_publish(&self) -> bool {
        self.pending.len() < self.capacity
    }

    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// # Errors
    /// Fails for a scene that does not fit in the mailbox, or that is not newer than the last published one.
    pub fn publish(&mut self, scene: CompletedScene<T>, completion: C) -> Result<(), Error> {
        if !self.can_publish() {
            return Err(Error::SceneMailboxFull);
        }
        if scene.generation <= self.last_published {
            return Err(Error::StaleScene {
                generation: scene.generation,
                published: self.last_published,
            });
        }
        self.last_published = scene.generation;
        self.pending.push_back(Pending { scene, completion });
        Ok(())
    }

    /// Removes from the pending scenes every scene for which `status` returns `Status::Complete`, and makes the newest
    /// of them the latest scene. A completed scene older than the latest scene is dropped.
    ///
    /// # Errors
    /// Fails with the first error returned by `status`. The scene whose status failed and every scene after it are
    /// still pending afterwards.
    pub fn poll<E>(&mut self, mut status: impl FnMut(&C) -> Result<Status, E>) -> Result<(), E> {
        let mut index = 0;
        while let Some(pending) = self.pending.get(index) {
            match status(&pending.completion)? {
                Status::Pending => index += 1,
                Status::Complete => {
                    if let Some(done) = self.pending.remove(index) {
                        self.show_if_newer(done.scene);
                    }
                }
            }
        }
        Ok(())
    }

    fn show_if_newer(&mut self, scene: CompletedScene<T>) {
        let newer =
            (self.latest.as_ref()).is_none_or(|latest| scene.generation > latest.generation);
        if newer {
            self.latest = Some(Arc::new(scene));
        }
    }

    /// The newest scene whose GPU work is complete. The returned scene can still be read after a newer scene is
    /// published.
    #[must_use]
    pub fn latest(&self) -> Option<Arc<CompletedScene<T>>> {
        self.latest.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    fn scene(generation: u64) -> CompletedScene<u64> {
        CompletedScene {
            generation,
            produced_at: Duration::ZERO,
            content: generation,
        }
    }

    fn status(complete: bool) -> Status {
        if complete {
            Status::Complete
        } else {
            Status::Pending
        }
    }

    #[test]
    fn pending_frames_are_bounded_and_only_completed_images_are_presented() -> Result<(), TestError>
    {
        let mut mailbox = SceneMailbox::new(2);
        assert!(mailbox.publish(scene(1), false).is_ok());
        assert!(mailbox.publish(scene(2), true).is_ok());
        assert!(mailbox.publish(scene(3), true).is_err());
        assert!(mailbox.latest().is_none());
        mailbox.poll(|complete| Ok::<_, Error>(status(*complete)))?;
        assert_eq!(mailbox.latest().map(|scene| scene.generation), Some(2));
        assert!(mailbox.can_publish());
        assert!(mailbox.publish(scene(1), true).is_err());
        mailbox.poll(|_| Ok::<_, Error>(Status::Complete))?;
        assert_eq!(mailbox.pending(), 0);
        assert_eq!(mailbox.latest().map(|scene| scene.generation), Some(2));
        Ok(())
    }

    #[test]
    fn presentation_lease_survives_publication_and_producer_stalls() -> Result<(), TestError> {
        let mut mailbox = SceneMailbox::new(2);
        assert!(mailbox.publish(scene(1), true).is_ok());
        mailbox.poll(|complete| Ok::<_, Error>(status(*complete)))?;
        let lease = mailbox
            .latest()
            .ok_or(TestError::Missing("published scene"))?;
        assert!(mailbox.publish(scene(2), false).is_ok());
        mailbox.poll(|complete| Ok::<_, Error>(status(*complete)))?;
        assert_eq!(mailbox.latest().map(|scene| scene.generation), Some(1));
        mailbox.poll(|_| Ok::<_, Error>(Status::Complete))?;
        assert_eq!(lease.content, 1);
        assert_eq!(mailbox.latest().map(|scene| scene.content), Some(2));
        Ok(())
    }

    #[test]
    fn a_failed_status_keeps_the_scenes_it_did_not_read() {
        let mut mailbox = SceneMailbox::new(2);
        assert!(mailbox.publish(scene(1), true).is_ok());
        assert!(mailbox.publish(scene(2), true).is_ok());
        assert_eq!(mailbox.poll(|_| Err("lost")), Err("lost"));
        assert_eq!(mailbox.pending(), 2);
        assert!(mailbox.latest().is_none());
    }

    #[test]
    fn a_refused_scene_reports_why_the_mailbox_refused_it() -> Result<(), TestError> {
        let mut mailbox = SceneMailbox::new(1);
        assert!(mailbox.publish(scene(2), false).is_ok());
        assert!(matches!(
            mailbox.publish(scene(3), false),
            Err(Error::SceneMailboxFull)
        ));
        mailbox.poll(|_| Ok::<_, Error>(Status::Complete))?;
        assert!(matches!(
            mailbox.publish(scene(1), false),
            Err(Error::StaleScene {
                generation: 1,
                published: 2
            })
        ));
        Ok(())
    }

    #[test]
    fn a_smaller_capacity_keeps_the_pending_scenes_until_they_complete() -> Result<(), TestError> {
        let mut mailbox = SceneMailbox::new(2);
        assert!(mailbox.publish(scene(1), false).is_ok());
        assert!(mailbox.publish(scene(2), false).is_ok());
        mailbox.set_capacity(1);
        assert!(!mailbox.can_publish());
        assert_eq!(mailbox.pending(), 2);
        mailbox.poll(|_| Ok::<_, Error>(Status::Complete))?;
        assert!(mailbox.can_publish());
        assert_eq!(mailbox.latest().map(|scene| scene.generation), Some(2));
        Ok(())
    }

    #[test]
    fn a_mailbox_without_capacity_refuses_every_scene() {
        let mut mailbox = SceneMailbox::<u64, bool>::new(0);
        assert!(!mailbox.can_publish());
        assert!(mailbox.publish(scene(1), true).is_err());
    }
}
