// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn card() -> RoundedRect {
    RoundedRect::new(
        Rect::new(20.dp(), 10.dp(), 160.dp(), 100.dp()),
        Corners::all(24.dp()),
    )
}

fn lens(rect: Rect) -> RoundedRect {
    RoundedRect::new(rect, Corners::all(Dp::INFINITY))
}

#[test]
fn fields_of_shapes_with_a_hole_match_shape_distance() -> Result<(), TestError> {
    let holes = [
        Rect::new(60.dp(), 40.dp(), 80.dp(), 40.dp()),
        Rect::new(150.dp(), 30.dp(), 60.dp(), 40.dp()),
        Rect::new(10.dp(), 0.dp(), 180.dp(), 120.dp()),
    ];
    for hole in holes {
        let shape = card().minus(lens(hole));
        let field = exact(&shape)?;
        for (index, p) in points() {
            let expected = shape.distance(p);
            assert!(
                (field[index] - expected).abs() <= 1e-3 + expected.abs() * 1e-5,
                "{shape:?} at {p:?}: shader {} and CPU {expected}",
                field[index]
            );
        }
    }
    Ok(())
}
