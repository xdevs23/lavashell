// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::units::DpExt;

#[test]
fn a_shape_receives_input_in_its_cells_unless_it_passes_input_through() -> Result<(), PointsError> {
    let square = Rect::new(0.dp(), 0.dp(), 2.dp(), 2.dp());
    let shape = WindowShape::rounded_rect(move |_| Some((square, 0.dp())));
    let size = Size::new(4.dp(), 4.dp());
    let cells = move |_: &SurfaceOutline| Ok::<_, PointsError>(vec![square]);
    let mut receiving = SurfaceShape::new(Some(shape.clone()));
    assert_eq!(
        receiving.update(size, Density::ONE, cells)?,
        [
            SurfaceChange::InputRegion(Some(vec![square])),
            SurfaceChange::BlurRegion(Some(vec![square])),
        ]
    );
    assert_eq!(receiving.update(size, Density::ONE, cells)?, []);
    assert!(receiving.accepts_input());
    let mut passthrough = SurfaceShape::new(Some(shape.input_passthrough()));
    assert_eq!(
        passthrough.update(size, Density::ONE, cells)?,
        [
            SurfaceChange::InputRegion(Some(Vec::new())),
            SurfaceChange::BlurRegion(Some(vec![square])),
        ]
    );
    assert!(!passthrough.accepts_input());
    Ok(())
}

#[test]
fn without_a_window_shape_only_the_blur_region_follows_the_size() -> Result<(), PointsError> {
    let mut whole = SurfaceShape::default();
    let size = Size::new(4.dp(), 4.dp());
    let cells = |_: &SurfaceOutline| Ok::<_, PointsError>(Vec::new());
    assert_eq!(
        whole.update(size, Density::ONE, cells)?,
        [SurfaceChange::BlurRegion(Some(vec![Rect::from_size(size)]))]
    );
    assert_eq!(whole.update(size, Density::ONE, cells)?, []);
    assert!(whole.accepts_input() && !whole.clips_paint());
    Ok(())
}

#[test]
fn contours_follow_the_size_of_the_surface() -> Result<(), PointsError> {
    let shape = WindowShape::new(|size| {
        vec![vec![
            Point::default(),
            Point::new(size.width, 0.dp()),
            Point::new(0.dp(), size.height),
        ]]
    });
    let size = Size::new(30.dp(), 20.dp());
    let triangle = Polygon::new([
        Point::default(),
        Point::new(30.dp(), 0.dp()),
        Point::new(0.dp(), 20.dp()),
    ])?;
    assert_eq!(
        shape.outline(size)?,
        SurfaceOutline::Contours(vec![triangle])
    );
    Ok(())
}

#[test]
fn an_invalid_contour_fails_and_no_contours_are_an_empty_outline() {
    let size = Size::new(30.dp(), 20.dp());
    let point = WindowShape::new(|_| vec![vec![Point::default()]]);
    assert_eq!(point.outline(size), Err(PointsError::TooFew));
    let nothing = WindowShape::new(|_| Vec::new());
    assert_eq!(nothing.outline(size), Ok(SurfaceOutline::Empty));
}

#[test]
fn a_missing_rounded_rectangle_is_an_empty_outline() -> Result<(), PointsError> {
    let rect = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
    let shape =
        WindowShape::rounded_rect(move |size| (size.width > 5.dp()).then_some((rect, 2.dp())));
    let rounded = RoundedRect::new(rect, Corners::all(2.dp()));
    let size = |width| Size::new(width, 10.dp());
    assert_eq!(
        shape.outline(size(10.dp()))?,
        SurfaceOutline::Rounded(rounded)
    );
    assert_eq!(shape.outline(size(1.dp()))?, SurfaceOutline::Empty);
    Ok(())
}

#[test]
fn passthrough_clips_without_input_and_input_only_takes_input_without_clipping() {
    let shape = WindowShape::new(|_| Vec::new());
    let passthrough = shape.clone().input_passthrough();
    assert!(passthrough.clips_paint() && !passthrough.accepts_input());
    let input_only = shape.input_only();
    assert!(!input_only.clips_paint() && input_only.accepts_input());
}
