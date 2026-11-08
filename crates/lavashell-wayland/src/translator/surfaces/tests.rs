// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::cell::Cell;
use std::rc::Rc;

const FIRST: SurfaceId = SurfaceId(1);
const SECOND: SurfaceId = SurfaceId(2);

/// A native surface that records when it is dropped.
struct Native(Rc<Cell<bool>>);

impl Drop for Native {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn native() -> (Native, Rc<Cell<bool>>) {
    let dropped = Rc::new(Cell::new(false));
    (Native(dropped.clone()), dropped)
}

#[test]
fn a_retired_surface_is_not_live_and_remains_until_its_release() {
    let mut surfaces = NativeSurfaces::default();
    let (surface, dropped) = native();
    surfaces.insert(FIRST, surface);
    assert!(surfaces.is_live(FIRST));
    assert_eq!(surfaces.find_live(|_| true), Some(FIRST));
    assert!(surfaces.retire(FIRST).is_some());
    assert!(!surfaces.is_live(FIRST));
    assert!(surfaces.live(FIRST).is_none());
    assert_eq!(surfaces.find_live(|_| true), None);
    assert!(surfaces.retire(FIRST).is_none());
    assert!(!dropped.get());
    drop(surfaces.release(FIRST));
    assert!(dropped.get());
    assert!(surfaces.release(FIRST).is_none());
}

#[test]
fn a_live_surface_is_not_released() {
    let mut surfaces = NativeSurfaces::default();
    let (surface, dropped) = native();
    surfaces.insert(FIRST, surface);
    assert!(surfaces.release(FIRST).is_none());
    assert!(surfaces.is_live(FIRST));
    assert!(!dropped.get());
}

#[test]
fn clearing_drops_the_live_and_the_retired_surfaces() {
    let mut surfaces = NativeSurfaces::default();
    let (live, live_dropped) = native();
    let (retired, retired_dropped) = native();
    surfaces.insert(FIRST, live);
    surfaces.insert(SECOND, retired);
    assert!(surfaces.retire(SECOND).is_some());
    surfaces.clear();
    assert!(live_dropped.get() && retired_dropped.get());
}
