// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn centered_visual_scale_keeps_layout_but_transforms_input_and_accessibility() -> TestResult {
    let scale = State::new(crate::constant!(Positive::try_new(0.5)));
    let value = scale.clone();
    let hits = State::new(0);
    let clicked = hits.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let clicked = clicked.clone();
            ui.layout(
                Axis::Vertical,
                Modifier::new()
                    .visual_scale(value.get())
                    .size(100.dp(), 40.dp())
                    .clip(Corners::all(4.dp())),
                |ui| {
                    ui.spacer(
                        100.dp(),
                        40.dp(),
                        Modifier::new()
                            .tag("scaled-child")
                            .on_click(move || clicked.set(clicked.get() + 1)),
                    );
                },
            );
            ui.spacer(20.dp(), 40.dp(), Modifier::new().tag("sibling"));
        });
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(200.dp(), 80.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(view.tagged_bounds("sibling")?.x, 100.dp());
    assert_eq!(
        view.tagged_bounds("scaled-child")?,
        Rect::new(25.dp(), 10.dp(), 50.dp(), 20.dp())
    );
    for (point, expected) in [
        (Point::new(5.dp(), 20.dp()), 0),
        (Point::new(50.dp(), 20.dp()), 1),
    ] {
        view.input(InputEvent::Press(point));
        view.input(InputEvent::Release(point));
        assert_eq!(hits.get(), expected);
    }
    assert!(
        view.accessibility()
            .nodes
            .values()
            .any(|node| node.bounds == Rect::new(25.dp(), 10.dp(), 50.dp(), 20.dp()))
    );
    scale.set(crate::constant!(Positive::try_new(1.0)));
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(200.dp(), 80.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.tagged_bounds("scaled-child")?,
        Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp())
    );
    assert_eq!(view.tagged_bounds("sibling")?.x, 100.dp());
    Ok(())
}
