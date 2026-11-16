// SPDX-License-Identifier: GPL-3.0-only
use super::catalog::{AlbumId, Playlist, TrackId};
use rand::{RngExt, rngs::SmallRng};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    Collection,
    Favorites,
    Recent,
    Playlist(Playlist),
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

#[derive(Clone)]
pub struct Model {
    pub current: TrackId,
    pub playing: bool,
    pub favorites: BTreeSet<TrackId>,
    pub query: String,
    pub section: Section,
    pub album: Option<AlbumId>,
    pub recent: Vec<TrackId>,
    pub position: Duration,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: bool,
    shuffle_order: SmallRng,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            current: TrackId::at(2),
            playing: false,
            favorites: BTreeSet::from([TrackId::at(0), TrackId::at(2), TrackId::at(5)]),
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
    pub fn toggle_favorite(&mut self, id: TrackId) {
        if !self.favorites.remove(&id) {
            let _absent_after_the_failed_remove = self.favorites.insert(id);
        }
    }

    pub fn select(&mut self, id: TrackId) {
        self.current = id;
        self.playing = true;
        self.position = Duration::ZERO;
        self.recent.retain(|recent| *recent != id);
        self.recent.insert(0, id);
    }

    pub fn advance(&mut self, elapsed: Duration) {
        self.position += elapsed;
        if self.position >= self.current.track().duration {
            if self.repeat {
                self.position = Duration::ZERO;
            } else {
                self.next();
            }
        }
    }

    #[must_use]
    pub fn played(&self) -> f64 {
        self.position.as_secs_f64() / self.current.track().duration.as_secs_f64()
    }

    pub fn next(&mut self) {
        self.skip(Direction::Forward);
    }

    pub fn previous(&mut self) {
        self.skip(Direction::Backward);
    }

    fn skip(&mut self, direction: Direction) {
        let queue: Vec<_> = TrackId::all()
            .filter(|id| self.album.is_none_or(|album| album == id.track().album))
            .collect();
        if queue.is_empty() {
            return;
        }
        let position = queue.iter().position(|id| *id == self.current);
        let step = if self.shuffle && queue.len() > 1 {
            self.shuffle_order.random_range(1..queue.len())
        } else {
            1
        };
        let new = match (position, direction) {
            (Some(old), Direction::Forward) => (old + step) % queue.len(),
            (Some(old), Direction::Backward) => {
                (old + queue.len() - step % queue.len()) % queue.len()
            }
            (None, Direction::Forward) => 0,
            (None, Direction::Backward) => queue.len() - 1,
        };
        self.select(queue[new]);
    }

    pub(super) fn visible(&self) -> Vec<TrackId> {
        let query = self.query.to_lowercase();
        let candidates = match self.section {
            Section::Recent => self.recent.clone(),
            Section::Collection | Section::Favorites | Section::Playlist(_) => {
                TrackId::all().collect()
            }
        };
        candidates
            .into_iter()
            .filter(|id| {
                let track = id.track();
                let album = track.album.album();
                let section = match self.section {
                    Section::Collection | Section::Recent => true,
                    Section::Favorites => self.favorites.contains(id),
                    Section::Playlist(playlist) => track.album == playlist.album,
                };
                section
                    && self.album.is_none_or(|selected| selected == track.album)
                    && (query.is_empty()
                        || format!("{} {} {}", track.title, album.title, album.artist)
                            .to_lowercase()
                            .contains(&query))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
