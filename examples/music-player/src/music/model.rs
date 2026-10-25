// SPDX-License-Identifier: GPL-3.0-only
use super::catalog::{ALBUMS, TRACKS};
use rand::{RngExt, rngs::SmallRng};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    Collection,
    Favorites,
    Recent,
    Playlist(usize),
}

/// Which way a skip moves through the queue.
#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

#[derive(Clone)]
pub struct Model {
    pub current: usize,
    pub playing: bool,
    pub favorites: BTreeSet<usize>,
    pub query: String,
    pub section: Section,
    pub album: Option<usize>,
    pub recent: Vec<usize>,
    pub position: Duration,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: bool,
    shuffle_order: SmallRng,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            current: 2,
            playing: false,
            favorites: BTreeSet::from([0, 2, 5]),
            query: String::new(),
            section: Section::Collection,
            album: None,
            recent: vec![],
            position: Duration::from_secs(83),
            volume: 0.7,
            shuffle: false,
            repeat: false,
            shuffle_order: rand::make_rng(),
        }
    }
}

impl Model {
    pub fn toggle_favorite(&mut self, index: usize) {
        if !self.favorites.remove(&index) {
            let _absent_after_the_failed_remove = self.favorites.insert(index);
        }
    }

    pub fn select(&mut self, index: usize) {
        self.current = index;
        self.playing = true;
        self.position = Duration::ZERO;
        self.recent.retain(|i| *i != index);
        self.recent.insert(0, index);
    }

    pub fn advance(&mut self, elapsed: Duration) {
        self.position += elapsed;
        if self.position >= TRACKS[self.current].duration {
            if self.repeat {
                self.position = Duration::ZERO;
            } else {
                self.next();
            }
        }
    }

    /// The part of the current track that has played, from 0 to 1.
    #[must_use]
    pub fn played(&self) -> f64 {
        self.position.as_secs_f64() / TRACKS[self.current].duration.as_secs_f64()
    }

    pub fn next(&mut self) {
        self.skip(Direction::Forward);
    }

    pub fn previous(&mut self) {
        self.skip(Direction::Backward);
    }

    /// Selects the track one step away in the queue. The queue contains the tracks of the selected album, or every
    /// track when no album is selected. With shuffle on, the step is a random number of tracks.
    fn skip(&mut self, direction: Direction) {
        let queue: Vec<_> = TRACKS
            .iter()
            .enumerate()
            .filter(|(_, t)| self.album.is_none_or(|a| a == t.album))
            .map(|(i, _)| i)
            .collect();
        if queue.is_empty() {
            return;
        }
        let old = queue.iter().position(|i| *i == self.current).unwrap_or(0);
        let step = if self.shuffle && queue.len() > 1 {
            self.shuffle_order.random_range(1..queue.len())
        } else {
            1
        };
        let new = match direction {
            Direction::Forward => (old + step) % queue.len(),
            Direction::Backward => (old + queue.len() - step % queue.len()) % queue.len(),
        };
        self.select(queue[new]);
    }

    pub(super) fn visible(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        let mut ids: Vec<_> = TRACKS
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                let a = &ALBUMS[t.album];
                let section = match self.section {
                    Section::Collection => true,
                    Section::Favorites => self.favorites.contains(i),
                    Section::Recent => self.recent.contains(i),
                    Section::Playlist(p) => t.album == p % ALBUMS.len(),
                };
                section
                    && self.album.is_none_or(|album| album == t.album)
                    && (query.is_empty()
                        || format!("{} {} {}", t.title, a.title, a.artist)
                            .to_lowercase()
                            .contains(&query))
            })
            .map(|(i, _)| i)
            .collect();
        if self.section == Section::Recent {
            ids.sort_by_key(|i| {
                self.recent
                    .iter()
                    .position(|r| r == i)
                    .unwrap_or(usize::MAX)
            });
        }
        ids
    }
}

#[cfg(test)]
mod tests;
