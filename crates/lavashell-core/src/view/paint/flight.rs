// SPDX-License-Identifier: GPL-3.0-only
use super::detached::OpenDetached;
use crate::scene::Transform;
use crate::ui::NodeId;
use crate::view::View;
use crate::view::collected::Collected;
use crate::view::crossing::Crossed;
use crate::view::output::GroupId;

pub(super) struct OpenFlight {
    flown: Option<NodeId>,
    painting: OpenDetached,
    outside: Collected,
}

impl View {
    pub(super) fn open_flight(&mut self, index: usize, parent: Transform) -> Option<OpenFlight> {
        let flown = match self.crossings.crossed(index)? {
            Crossed::Omitted => None,
            Crossed::Flown(_) => Some(self.nodes[index].id),
            Crossed::Blended(_) => return None,
        };
        Some(OpenFlight {
            flown,
            painting: self.open_detached(parent),
            outside: self.isolate(),
        })
    }

    pub(super) fn close_flight(&mut self, open: OpenFlight) {
        let inside = self.restore(open.outside);
        self.keep_painted(inside);
        let painting = self.close_detached(open.painting);
        (self.painting.flown).extend(open.flown.map(|id| (id, painting)));
    }

    pub(super) fn paint_flights_around(&mut self, around: usize, inner: Transform) {
        let flights = self.crossings.take_flights_around(around);
        let flown = self.flown_rects(&flights);
        let into_around = self.routing.moving.at_rest(inner).inverse();
        for (flight, rect) in flights.values().zip(flown.into_values()) {
            let (from, to) = (flight.pair.from, flight.pair.to);
            let moved = Transform::between(self.layout[to].rect, rect);
            let id = self.nodes[to].id;
            if let Some(painting) = self.painting.flown.remove(&id) {
                let placed = painting.placed(GroupId::Flight(id), into_around.then(moved));
                self.painting.canvas.nest(placed);
            }
            self.layout[from].rect = rect;
            self.layout[to].rect = rect;
        }
    }
}
