// SPDX-License-Identifier: GPL-3.0-only
mod flight;

use super::View;
use super::layout::Visibility;
use super::transition::VisualAppearance;
use crate::animation::Animation;
use crate::geometry::{Point, Rect};
use crate::rebuilt::BuiltLike;
use crate::ui::{Identity, SharedKey, ViewId};
use std::cell::LazyCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct Crossing {
    pub from: ViewId,
    pub to: ViewId,
    pub progress: Animation<f64>,
}

impl Crossing {
    fn progress_in_flight(&self) -> Option<f64> {
        let progress = self.progress.get();
        (!matches!(progress, 0.0 | 1.0)).then_some(progress)
    }
}

impl BuiltLike for Crossing {
    fn is_built_like(&self, earlier: &Self) -> bool {
        self.from == earlier.from
            && self.to == earlier.to
            && self.progress.is_built_like(&earlier.progress)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Crossed {
    Omitted,
    Flown(VisualAppearance),
    Blended(VisualAppearance),
}

pub(super) struct Flight {
    pub pair: Pair,
    pub outer: Option<Pair>,
    pub progress: f64,
}

#[derive(Default)]
pub(super) struct Crossings {
    crossed: BTreeMap<usize, Crossed>,
    flights_around: BTreeMap<usize, BTreeMap<usize, Flight>>,
}

impl Crossings {
    pub fn crossed(&self, index: usize) -> Option<&Crossed> {
        self.crossed.get(&index)
    }

    pub fn appearance(&self, index: usize) -> Option<&VisualAppearance> {
        match self.crossed(index)? {
            Crossed::Flown(appearance) | Crossed::Blended(appearance) => Some(appearance),
            Crossed::Omitted => None,
        }
    }

    pub fn take_flights_around(&mut self, index: usize) -> BTreeMap<usize, Flight> {
        self.flights_around.remove(&index).unwrap_or_default()
    }
}

type SharedInViews = BTreeMap<ViewId, BTreeMap<SharedKey, usize>>;

impl View {
    pub(super) fn crossings_in_flight(&self) -> Crossings {
        let shared = LazyCell::new(|| self.shown_shared_nodes());
        let mut crossings = Crossings::default();
        for crossing in self.nodes.iter().flat_map(|node| &node.crossings) {
            if let Some(progress) = crossing.progress_in_flight() {
                self.add_flights(&mut crossings, &pairs_of(&shared, crossing), progress);
            }
        }
        crossings
    }

    fn shown_shared_nodes(&self) -> SharedInViews {
        let shown = (self.nodes.iter().zip(&self.layout).enumerate())
            .filter(|(_, (_, layout))| layout.visibility == Visibility::Shown);
        let mut shared = SharedInViews::new();
        for (index, (node, _)) in shown {
            if let Identity::Shared { view, key } = node.identity {
                let _none_as_each_id_is_unique = shared.entry(view).or_default().insert(key, index);
            }
        }
        shared
    }

    fn add_flights(&self, crossings: &mut Crossings, pairs: &[Pair], progress: f64) {
        let paired: BTreeSet<usize> = pairs.iter().map(|pair| pair.to).collect();
        let arounds = (pairs.iter()).filter_map(|&pair| Some((pair, self.around(pair)?)));
        for (pair, around) in arounds {
            let flown = Crossed::Flown(self.blended(pair, progress));
            let inside = (self.matched_descendants(pair, &paired).into_iter())
                .map(|inside| (inside.to, Crossed::Blended(self.blended(inside, progress))));
            (crossings.crossed).extend([(pair.from, Crossed::Omitted), (pair.to, flown)]);
            crossings.crossed.extend(inside);
            let outer = self.outer_of(pair, pairs);
            let flight = Flight {
                pair,
                outer,
                progress,
            };
            let flights = crossings.flights_around.entry(around).or_default();
            let _none_as_each_pair_has_its_own_target = flights.insert(pair.to, flight);
        }
    }

    fn blended(&self, pair: Pair, progress: f64) -> VisualAppearance {
        let from = VisualAppearance::of(&self.nodes[pair.from]);
        let to = VisualAppearance::of(&self.nodes[pair.to]);
        flight::blend(&from, &to, progress)
    }

    fn around(&self, pair: Pair) -> Option<usize> {
        let above_to: BTreeSet<usize> = self.parents.ancestry(pair.to).collect();
        self.parents
            .ancestry(pair.from)
            .find(|node| above_to.contains(node))
    }

    fn matched_descendants(&self, pair: Pair, paired: &BTreeSet<usize>) -> Vec<Pair> {
        let children = |index: usize| self.nodes[index].children.iter().copied();
        (children(pair.from).zip(children(pair.to)))
            .map(|(from, to)| Pair { from, to })
            .filter(|inside| !paired.contains(&inside.to))
            .flat_map(|inside| {
                std::iter::once(inside).chain(self.matched_descendants(inside, paired))
            })
            .collect()
    }

    fn outer_of(&self, pair: Pair, pairs: &[Pair]) -> Option<Pair> {
        let outer = (self.parents.ancestry(pair.from).skip(1))
            .find_map(|ancestor| pairs.iter().find(|outer| outer.from == ancestor))?;
        (self.parents.ancestry(pair.to).any(|node| node == outer.to)).then_some(*outer)
    }

    pub(super) fn flown_rects(&self, flights: &BTreeMap<usize, Flight>) -> BTreeMap<usize, Rect> {
        let mut flown = BTreeMap::new();
        for flight in flights.values() {
            let start = (flight.outer)
                .and_then(|outer| flown.get(&outer.to))
                .map_or(Point::default(), |outer: &Rect| outer.origin());
            let inside = |index: usize, outer: Option<usize>| {
                let start =
                    outer.map_or(Point::default(), |outer| self.layout[outer].rect.origin());
                self.layout[index]
                    .rect
                    .translated(Point::new(-start.x, -start.y))
            };
            let (pair, outer) = (flight.pair, flight.outer);
            let from = inside(pair.from, outer.map(|outer| outer.from));
            let to = inside(pair.to, outer.map(|outer| outer.to));
            let rect = flight::blend(&from, &to, flight.progress).translated(start);
            let _none_as_each_pair_has_its_own_target = flown.insert(pair.to, rect);
        }
        flown
    }
}

fn pairs_of(shared: &SharedInViews, crossing: &Crossing) -> Vec<Pair> {
    let (Some(from), Some(to)) = (shared.get(&crossing.from), shared.get(&crossing.to)) else {
        return Vec::new();
    };
    (from.iter())
        .filter_map(|(key, &from)| {
            Some(Pair {
                from,
                to: *to.get(key)?,
            })
        })
        .collect()
}
