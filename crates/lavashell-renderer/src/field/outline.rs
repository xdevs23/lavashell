// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::paint::skpath;
use crate::retention::{Frames, Retained};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::ContourDirection;
use i_overlay::core::solver::Solver;
use i_overlay::float::overlay::OverlayOptions;
use i_overlay::float::simplify::SimplifyShape;
use lavashell_core::prelude::*;
use skia_safe::Path;
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

pub(crate) struct OutlinePath {
    pub path: Path,
    regions: RefCell<Retained<[u64; 8], Option<Path>>>,
}

impl OutlinePath {
    pub fn new(path: Path) -> Self {
        Self {
            path,
            regions: RefCell::default(),
        }
    }

    pub fn region(
        &self,
        frames: &Frames,
        band: [u64; 8],
        make: impl FnOnce(&Path) -> Option<Path>,
    ) -> Option<Path> {
        let mut regions = self.regions.borrow_mut();
        regions
            .get_or_make(band, frames, |_| make(&self.path))
            .clone()
    }

    pub fn retain_recent(&self, frames: &Frames) {
        self.regions.borrow_mut().retain_recent(frames);
    }
}

pub(crate) trait Outlined {
    fn outline(&self) -> Option<&OutlinePath>;
}

impl Outlined for Rc<ContourPath> {
    fn outline(&self) -> Option<&OutlinePath> {
        self.outline.as_deref().ok()
    }
}

impl Outlined for Result<Option<Rc<OutlinePath>>, DrawFailure> {
    fn outline(&self) -> Option<&OutlinePath> {
        self.as_ref().ok()?.as_deref()
    }
}

pub(crate) struct OutlinePaths<K, V>(RefCell<Retained<K, V>>);

impl<K, V> Default for OutlinePaths<K, V> {
    fn default() -> Self {
        Self(RefCell::default())
    }
}

impl<K: Eq + Hash, V: Clone + Outlined> OutlinePaths<K, V> {
    pub fn get_or_make(&self, key: K, frames: &Frames, make: impl FnOnce() -> V) -> V {
        self.0
            .borrow_mut()
            .get_or_make(key, frames, |_| make())
            .clone()
    }

    pub fn retain_recent(&mut self, frames: &Frames) {
        let paths = self.0.get_mut();
        paths.retain_recent(frames);
        for outline in paths.values().filter_map(Outlined::outline) {
            outline.retain_recent(frames);
        }
    }
}

pub(crate) struct ContourPath {
    pub outline: Result<Rc<OutlinePath>, DrawFailure>,
    segments: Vec<f64>,
}

impl ContourPath {
    pub fn new(polygon: &Polygon) -> Self {
        let points = (polygon.points().iter())
            .map(|p| [p.x.get(), p.y.get()])
            .collect::<Vec<_>>();
        let mut options = OverlayOptions::default();
        options.output_direction = ContourDirection::CounterClockwise;
        options.preserve_input_collinear = true;
        options.preserve_output_collinear = true;
        let filled = points.simplify_shape_custom(FillRule::EvenOdd, options, Solver::default());
        let segments = (filled.iter().flatten())
            .flat_map(|contour| contour.iter().zip(contour.iter().cycle().skip(1)))
            .flat_map(|(a, b)| [a[0], a[1], b[0], b[1]])
            .collect();
        Self {
            outline: skpath(polygon).map(|path| Rc::new(OutlinePath::new(path))),
            segments,
        }
    }

    pub fn segments(&self) -> &[f64] {
        &self.segments
    }
}
