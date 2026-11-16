// SPDX-License-Identifier: GPL-3.0-only

use lavashell_core::surface::SurfaceId;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::hash::Hash;

#[derive(Default)]
pub(crate) struct Frames {
    begun: HashMap<SurfaceId, u64>,
    current: Option<BegunFrame>,
}

#[derive(Clone, Copy)]
struct BegunFrame {
    surface: SurfaceId,
    frame: u64,
}

impl Frames {
    pub fn begin(&mut self, surface: SurfaceId) {
        let frame = self.begun.entry(surface).or_default();
        *frame += 1;
        self.current = Some(BegunFrame {
            surface,
            frame: *frame,
        });
    }

    pub fn forget(&mut self, surface: SurfaceId) {
        let _forgotten_frame = self.begun.remove(&surface);
    }
}

#[derive(Default)]
pub(crate) struct Uses(HashMap<SurfaceId, u64>);

impl Uses {
    pub fn mark(&mut self, frames: &Frames) {
        if let Some(BegunFrame { surface, frame }) = frames.current {
            let _earlier_frame = self.0.insert(surface, frame);
        }
    }

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

    pub fn get(&mut self, source: &K, frames: &Frames) -> Option<&V> {
        let (value, uses) = self.entries.get_mut(source)?;
        uses.mark(frames);
        Some(value)
    }

    pub fn insert(&mut self, source: K, value: V, frames: &Frames) {
        let mut uses = Uses::default();
        uses.mark(frames);
        self.entries.extend([(source, (value, uses))]);
    }

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
        frames.begin(SurfaceId::numbered(1));
        assert!(made(&mut retained, &frames));
        for _ in 0..3 {
            frames.begin(SurfaceId::numbered(2));
            retained.retain_recent(&frames);
            frames.begin(SurfaceId::numbered(1));
            retained.retain_recent(&frames);
            assert!(!made(&mut retained, &frames), "kept for its own surface");
        }
        frames.begin(SurfaceId::numbered(1));
        retained.retain_recent(&frames);
        frames.begin(SurfaceId::numbered(1));
        retained.retain_recent(&frames);
        assert!(
            made(&mut retained, &frames),
            "dropped after a frame without use"
        );
        frames.forget(SurfaceId::numbered(1));
        frames.begin(SurfaceId::numbered(2));
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
