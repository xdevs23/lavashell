// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::output_records::OutputRecords;
use lavashell_core::geometry::Size;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::units::{Density, DpExt};
use lavashell_platform::OutputEvent;
use std::cell::RefCell;
use std::rc::Rc;

const MONITOR: OutputId = OutputId(3);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the surface records failed")]
    Records(#[from] crate::surface_records::Error),
    #[error("the surface of the dock did not open")]
    DockRefused,
}

type TestResult = Result<(), TestError>;

fn info(width: i32) -> MonitorInfo {
    MonitorInfo {
        name: Some("DP-1".to_owned()),
        size: Size::new(width.dp(), 1080.dp()),
        physical_size: None,
    }
}

fn view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

/// The geometry states passed to a factory, in the order of the calls.
type Seen = Rc<RefCell<Vec<State<MonitorInfo>>>>;

/// Panels whose factory makes a bar and a dock, and records the geometry state that it gets for each output.
fn bar_and_dock() -> (PanelsPerOutput<&'static str>, Seen) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorded = seen.clone();
    let panels = PanelsPerOutput::new(move |info| {
        recorded.borrow_mut().push(info);
        vec![("bar", view()), ("dock", view())]
    });
    (panels, seen)
}

/// Opens the surface of each panel in `surfaces`.
fn open_in(
    surfaces: &mut SurfaceRecords,
) -> impl FnMut(&'static str, View) -> Result<SurfaceId, TestError> {
    |_, view| {
        Ok(surfaces.open(
            view,
            Size::new(1920.dp(), 1.dp()),
            Density::ONE,
            None,
            String::new(),
        )?)
    }
}

#[test]
fn an_output_gets_its_panels_once_its_geometry_is_known() -> TestResult {
    let (mut panels, seen) = bar_and_dock();
    let mut outputs = OutputRecords::default();
    let mut opened = Vec::new();
    let mut record = |name: &'static str, _: View| {
        opened.push(name);
        Ok::<_, TestError>(SurfaceId(0))
    };
    outputs.handle(OutputEvent::Entered(MONITOR, SurfaceId(0)));
    panels.output_updated(MONITOR, outputs.info(MONITOR).cloned(), &mut record)?;
    outputs.handle(OutputEvent::Updated(MONITOR, info(1920)));
    panels.output_updated(MONITOR, outputs.info(MONITOR).cloned(), &mut record)?;
    assert_eq!(seen.borrow().len(), 1);
    outputs.handle(OutputEvent::Updated(MONITOR, info(2560)));
    panels.output_updated(MONITOR, outputs.info(MONITOR).cloned(), &mut record)?;
    assert_eq!(opened, ["bar", "dock"]);
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!((seen.borrow())[0].get(), info(2560));
    Ok(())
}

#[test]
fn the_panels_of_a_removed_output_close() -> TestResult {
    let other = OutputId(4);
    let (mut panels, _) = bar_and_dock();
    let mut surfaces = SurfaceRecords::new(PreferenceOverrides::default());
    for output in [MONITOR, other] {
        panels.output_updated(output, Some(info(1920)), open_in(&mut surfaces))?;
    }
    assert_eq!(
        panels.output_removed(MONITOR, &mut surfaces),
        [
            SurfaceRequest::Destroy(SurfaceId(0)),
            SurfaceRequest::Destroy(SurfaceId(1))
        ]
    );
    assert!(panels.output_removed(MONITOR, &mut surfaces).is_empty());
    assert!(surfaces.is_open(SurfaceId(2)));
    assert!(surfaces.is_open(SurfaceId(3)));
    Ok(())
}

#[test]
fn an_output_records_its_panels_only_after_every_surface_opened() -> TestResult {
    let (mut panels, seen) = bar_and_dock();
    let mut surfaces = SurfaceRecords::new(PreferenceOverrides::default());
    let failed = panels.output_updated(MONITOR, Some(info(1920)), |name, view| match name {
        "dock" => Err(TestError::DockRefused),
        _ => open_in(&mut surfaces)(name, view),
    });
    assert!(matches!(failed, Err(TestError::DockRefused)));
    assert!(panels.output_removed(MONITOR, &mut surfaces).is_empty());
    panels.output_updated(MONITOR, Some(info(1920)), open_in(&mut surfaces))?;
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(panels.output_removed(MONITOR, &mut surfaces).len(), 2);
    Ok(())
}
