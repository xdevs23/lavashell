// SPDX-License-Identifier: GPL-3.0-only
use super::*;

/// A view with a first-fitting container in a parent `width` wide: a face 120 dp wide that counts its clicks in
/// `clicks`, then the text "narrow".
fn faces(width: Dp, clicks: State<u32>) -> View {
    View::new(crate::platform::fixture::facts(), move |ui| {
        let clicks = clicks.clone();
        ui.layout(Arrangement::Overlay, Modifier::new().width(width), |ui| {
            ui.first_fitting(Modifier::new(), |ui| {
                let counted = Modifier::new()
                    .tag("wide")
                    .on_click(move || clicks.update(|count| *count += 1));
                ui.spacer(120.dp(), 20.dp(), counted);
                ui.text("narrow", Modifier::new());
            });
        });
    })
}

fn texts(commands: &[DrawCommand]) -> Vec<&str> {
    (commands.iter())
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn exports(view: &View, name: &str) -> bool {
    (view.accessibility().nodes.values()).any(|node| node.name.as_deref() == Some(name))
}

#[test]
fn the_first_face_that_fits_shows_alone() -> TestResult {
    let clicks = State::new(0);
    let mut view = faces(200.dp(), clicks.clone());
    let commands = painted(frame(&mut view, WINDOW_SIZE, 0)?)?;
    assert!(texts(&commands).is_empty());
    assert!(!exports(&view, "narrow"));
    tap(&mut view, "wide")?;
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn the_next_face_shows_where_the_first_does_not_fit() -> TestResult {
    let clicks = State::new(0);
    let mut view = faces(100.dp(), clicks.clone());
    let commands = painted(frame(&mut view, WINDOW_SIZE, 0)?)?;
    assert_eq!(texts(&commands), ["narrow"]);
    assert!(exports(&view, "narrow"));
    tap_at(&mut view, Point::new(110.dp(), 10.dp()));
    tap_at(&mut view, Point::new(10.dp(), 10.dp()));
    assert_eq!(clicks.get(), 0, "the hidden face ignores clicks");
    Ok(())
}

#[test]
fn the_last_face_shows_where_none_fits() -> TestResult {
    let mut view = faces(10.dp(), State::new(0));
    let commands = painted(frame(&mut view, WINDOW_SIZE, 0)?)?;
    assert_eq!(texts(&commands), ["narrow"]);
    Ok(())
}

#[test]
fn a_child_out_of_the_flow_shows_beside_the_face_that_fits() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Arrangement::Overlay,
            Modifier::new().width(100.dp()),
            |ui| {
                ui.first_fitting(Modifier::new(), |ui| {
                    ui.text("badge", Modifier::new().out_of_flow());
                    ui.spacer(120.dp(), 20.dp(), Modifier::new());
                    ui.text("narrow", Modifier::new());
                });
            },
        );
    });
    let commands = painted(frame(&mut view, WINDOW_SIZE, 0)?)?;
    assert_eq!(texts(&commands), ["badge", "narrow"]);
    Ok(())
}

#[test]
fn a_button_around_the_faces_is_named_by_the_face_it_shows() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Arrangement::Overlay,
            Modifier::new().width(100.dp()),
            |ui| {
                ui.first_fitting(Modifier::new().on_click(|| {}), |ui| {
                    ui.text("a long face", Modifier::new().width(120.dp()));
                    ui.text("short", Modifier::new());
                });
            },
        );
    });
    let _update = frame(&mut view, WINDOW_SIZE, 0)?;
    assert!(exports(&view, "short"));
    assert!(!exports(&view, "a long face short"));
    Ok(())
}
