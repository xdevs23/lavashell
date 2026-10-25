// SPDX-License-Identifier: GPL-3.0-only
use super::skpath_through;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use skia_safe::PathVerb;

#[test]
fn a_path_keeps_its_curves_and_closes_only_when_asked() -> Result<(), TestError> {
    let at = |x: f64, y: f64| Point::new(Dp::new(x), Dp::new(y));
    let segments = [
        Segment::Line(at(10.0, 0.0)),
        Segment::Quadratic {
            control: at(20.0, 0.0),
            to: at(20.0, 10.0),
        },
        Segment::Cubic {
            first_control: at(20.0, 20.0),
            second_control: at(10.0, 20.0),
            to: at(0.0, 10.0),
        },
    ];
    let open = skpath_through(at(0.0, 0.0), &segments, false)?;
    assert_eq!(
        open.verbs(),
        [
            PathVerb::Move,
            PathVerb::Line,
            PathVerb::Quad,
            PathVerb::Cubic
        ]
    );
    let closed = skpath_through(at(0.0, 0.0), &segments, true)?;
    assert_eq!(closed.verbs().last(), Some(&PathVerb::Close));
    Ok(())
}
