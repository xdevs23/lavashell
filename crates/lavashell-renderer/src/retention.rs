// SPDX-License-Identifier: GPL-3.0-only
//! Storage for values made from scene data, such as field textures and paths. A stored value is dropped once no
//! surface has used it in the frame that the surface paints or in the frame before.

use lavashell_core::surface::SurfaceId;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::hash::Hash;

#[derive(Default)]
pub(crate) struct Frames {
    begun: HashMap<SurfaceId, u64>,
    current: Option<(SurfaceId, u64)>,
}

impl Frames {
    pub fn begin(&mut self, surface: SurfaceId) {
        let frame = self.begun.entry(surface).or_default();
        *frame += 1;
        self.current = Some((surface, *frame));
    }

    pub fn forget(&mut self, surface: SurfaceId) {
        let _forgotten_frame = self.begun.remove(&surface);
    }
}

#[derive(Default)]
pub(crate) struct Uses(Vec<(SurfaceId, u64)>);

impl Uses {
    pub fn mark(&mut self, frames: &Frames) {
        let Some((surface, frame)) = frames.current else {
            return;
        };
        match self.0.iter_mut().find(|(used, _)| *used == surface) {
            Some(last) => last.1 = frame,
            None => self.0.push((surface, frame)),
        }
    }

    /// Whether some surface used the value in the frame that the surface paints
    /// now or in the frame before. Such a value is stored for the next frame.
    pub fn recent(&self, frames: &Frames) -> bool {
        self.0.iter().any(|(surface, frame)| {
            frames
                .begun
                .get(surface)
                .is_some_and(|begun| frame + 1 >= *begun)
        })
    }
}

pub(crate) struct Retained<K, V> {
    entries: HashMap<K, (V, Uses)>,
}

impl<K, V> Default for Retained<K, V> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, V> Retained<K, V> {
    pub fn get_or_make(
        &mut self,
        source: K,
        frames: &Frames,
        make: impl FnOnce(&K) -> V,
    ) -> &mut V {
        let Ok(value) = self.get_or_try_make(source, frames, |key| Ok::<_, Infallible>(make(key)));
        value
    }

    /// The value stored for `source`, marked as used. Calls `make` when no value is stored. When `make` fails,
    /// nothing is stored.
    pub fn get_or_try_make<E>(
        &mut self,
        source: K,
        frames: &Frames,
        make: impl FnOnce(&K) -> Result<V, E>,
    ) -> Result<&mut V, E> {
        let (value, uses) = match self.entries.entry(source) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let value = make(entry.key())?;
                entry.insert((value, Uses::default()))
            }
        };
        uses.mark(frames);
        Ok(value)
    }

    /// The value of `source`, marked as used, or None when there is none.
    pub fn get(&mut self, source: &K, frames: &Frames) -> Option<&V> {
        let (value, uses) = self.entries.get_mut(source)?;
        uses.mark(frames);
        Some(value)
    }

    /// Stores `value` for `source` in place of the value before it, marked as used.
    pub fn insert(&mut self, source: K, value: V, frames: &Frames) {
        let mut uses = Uses::default();
        uses.mark(frames);
        self.entries.extend([(source, (value, uses))]);
    }

    /// The value stored for `source` when `fits` accepts it, marked as used. Calls `make` when no value is stored or
    /// when `fits` rejects the stored value, and stores the result in its place.
    pub fn get_fitting_or_make(
        &mut self,
        source: K,
        frames: &Frames,
        fits: impl FnOnce(&V) -> bool,
        make: impl FnOnce() -> V,
    ) -> &mut V {
        let (value, uses) = match self.entries.entry(source) {
            Entry::Occupied(entry) => {
                let stored = entry.into_mut();
                if !fits(&stored.0) {
                    *stored = (make(), Uses::default());
                }
                stored
            }
            Entry::Vacant(entry) => entry.insert((make(), Uses::default())),
        };
        uses.mark(frames);
        value
    }

    pub fn retain_recent(&mut self, frames: &Frames) {
        self.entries.retain(|_, (_, uses)| uses.recent(frames));
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.values().map(|(value, _)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_stays_while_its_surface_uses_it_and_another_surface_paints_between() {
        let mut frames = Frames::default();
        let mut retained = Retained::<u32, u32>::default();
        let made = |retained: &mut Retained<u32, u32>, frames: &Frames| {
            let mut made = false;
            let value = *retained.get_or_make(1, frames, |_| {
                made = true;
                7
            });
            assert_eq!(value, 7);
            made
        };
        frames.begin(SurfaceId(1));
        assert!(made(&mut retained, &frames));
        for _ in 0..3 {
            frames.begin(SurfaceId(2));
            retained.retain_recent(&frames);
            frames.begin(SurfaceId(1));
            retained.retain_recent(&frames);
            assert!(!made(&mut retained, &frames), "kept for its own surface");
        }
        frames.begin(SurfaceId(1));
        retained.retain_recent(&frames);
        frames.begin(SurfaceId(1));
        retained.retain_recent(&frames);
        assert!(
            made(&mut retained, &frames),
            "dropped after a frame without use"
        );
        frames.forget(SurfaceId(1));
        frames.begin(SurfaceId(2));
        retained.retain_recent(&frames);
        assert_eq!(
            retained.values().count(),
            0,
            "a forgotten surface keeps nothing"
        );
    }

    #[test]
    fn a_stored_value_that_does_not_fit_is_made_again() {
        let frames = Frames::default();
        let mut retained = Retained::<u32, u32>::default();
        assert_eq!(
            *retained.get_fitting_or_make(1, &frames, |_| false, || 7),
            7
        );
        assert_eq!(
            *retained.get_fitting_or_make(1, &frames, |&value| value == 7, || 8),
            7
        );
        assert_eq!(
            *retained.get_fitting_or_make(1, &frames, |&value| value == 9, || 9),
            9
        );
        assert_eq!(retained.values().count(), 1);
    }
}
