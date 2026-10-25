// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scene::{Icon, IconPath, Segment};
use crate::ui::{IconOptions, IconStyle};

static SQUARE: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[IconPath {
        start: Point::new(Dp::new(4.0), Dp::new(4.0)),
        segments: &[
            Segment::Line(Point::new(Dp::new(20.0), Dp::new(4.0))),
            Segment::Line(Point::new(Dp::new(20.0), Dp::new(20.0))),
            Segment::Line(Point::new(Dp::new(4.0), Dp::new(20.0))),
        ],
        closed: true,
        stroke: Some(2.0),
    }],
};

#[test]
fn the_options_of_an_icon_give_its_size_and_whether_it_fills_its_closed_paths() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.text("", Modifier::new().tag("text"));
            ui.icon(&SQUARE, IconOptions::default(), Modifier::new().tag("line"));
            let filled = IconOptions {
                size: Some(40.dp()),
                style: IconStyle::Filled,
            };
            ui.icon(&SQUARE, filled, Modifier::new().tag("filled"));
        });
    });
    let fills = (painted(frame(&mut view, WINDOW_SIZE, 0)?)?.into_iter())
        .filter(|command| matches!(command, DrawCommand::Path { stroke: None, .. }))
        .count();
    let side = view.tagged_bounds("text")?.height;
    assert_eq!(
        view.bounds_of("line").map(Rect::size),
        Some(Size::new(side, side))
    );
    assert_eq!(
        view.bounds_of("filled").map(Rect::size),
        Some(Size::new(40.dp(), 40.dp()))
    );
    assert_eq!(fills, 1);
    Ok(())
}
