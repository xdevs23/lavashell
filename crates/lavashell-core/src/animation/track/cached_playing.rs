// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, Playing};
use std::rc::Rc;

/// The playing motion of a track, built again only when the revision of the track changes.
pub(super) struct CachedPlaying<T: Animatable> {
    revision: u64,
    playing: Option<Rc<Playing<T>>>,
}

impl<T: Animatable> Default for CachedPlaying<T> {
    fn default() -> Self {
        Self {
            revision: 0,
            playing: None,
        }
    }
}

impl<T: Animatable> CachedPlaying<T> {
    pub(super) fn get_or_build(
        &mut self,
        revision: u64,
        build: impl FnOnce() -> Option<Playing<T>>,
    ) -> Option<Rc<Playing<T>>> {
        if self.revision != revision {
            *self = Self {
                revision,
                playing: build().map(Rc::new),
            };
        }
        self.playing.clone()
    }
}
