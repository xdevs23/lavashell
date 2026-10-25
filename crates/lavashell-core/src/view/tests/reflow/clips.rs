// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_clips_of_a_scrolling_reflowing_node_follow_its_surface() -> TestResult {
    let wide = State::new(false);
    let target = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.key("island", Modifier::new(), |ui| {
            ui.scroll(
                Axis::Horizontal,
                Modifier::new()
                    .offset(20.dp(), 10.dp())
                    .clip_rect(Rect::new(0.dp(), 0.dp(), 220.dp(), 40.dp()), 0.dp())
                    .padding(10.dp())
                    .transition(Transition::new(linear()).reflow()),
                |ui| pieces(ui, target.get()),
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let clips: Vec<_> = (scene.commands().iter())
        .filter_map(|command| match command {
            DrawCommand::Clip { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(
        clips,
        [
            Rect::new(20.dp(), 10.dp(), 170.dp(), 40.dp()),
            Rect::new(30.dp(), 20.dp(), 150.dp(), 20.dp())
        ]
    );
    Ok(())
}

#[test]
fn a_placed_clip_follows_only_the_edges_of_a_reflowing_node_it_lies_on() -> TestResult {
    let wide = State::new(false);
    let target = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.key(
            "island",
            Modifier::new()
                .offset(20.dp(), 10.dp())
                .clip_rect(Rect::new(0.dp(), 0.dp(), 60.dp(), 20.dp()), 0.dp())
                .clip_rect(Rect::new(100.dp(), 0.dp(), 100.dp(), 20.dp()), 0.dp())
                .transition(Transition::new(linear()).reflow()),
            |ui| {
                ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                    pieces(ui, target.get());
                });
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let clips: Vec<_> = (scene.commands().iter())
        .filter_map(|command| match command {
            DrawCommand::Clip { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(
        clips,
        [
            Rect::new(20.dp(), 10.dp(), 60.dp(), 20.dp()),
            Rect::new(120.dp(), 10.dp(), 50.dp(), 20.dp())
        ]
    );
    Ok(())
}
