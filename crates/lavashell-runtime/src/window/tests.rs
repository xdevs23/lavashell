// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_text::NoText;
use lavashell_core::color::Gamut;
use lavashell_core::output::{OutputProfile, Precision};
use lavashell_core::prelude::Modifier;
use lavashell_core::units::DpExt;

fn spacer_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.spacer(120.dp(), 30.dp(), Modifier::new());
    })
}

#[test]
fn a_window_opens_at_its_requested_size_or_else_at_the_size_its_view_fits() -> Result<(), Error> {
    let mut view = spacer_view();
    let window = Window::new("Title", "org.example.Title");
    let measured = window.spec(&mut view, &mut NoText)?;
    assert_eq!(measured.size, Size::new(120.dp(), 30.dp()));
    let requested = window
        .size(400.dp(), 300.dp())
        .spec(&mut view, &mut NoText)?;
    assert_eq!(requested.size, Size::new(400.dp(), 300.dp()));
    Ok(())
}

#[test]
fn a_shaped_window_is_transparent_and_draws_its_decorations() -> Result<(), Error> {
    let mut view = spacer_view();
    let window = Window::new("Title", "org.example.Title");
    let plain = window.clone().spec(&mut view, &mut NoText)?;
    let shaped = window
        .shape(WindowShape::new(|_| Vec::new()))
        .spec(&mut view, &mut NoText)?;
    let draws_decorations = |spec: &SurfaceSpec| {
        let Role::Window {
            draws_decorations, ..
        } = spec.role;
        draws_decorations
    };
    assert!(!plain.appearance.transparent && !draws_decorations(&plain));
    assert!(shaped.appearance.transparent && draws_decorations(&shaped));
    Ok(())
}

#[test]
fn the_spec_carries_every_setting_of_the_window() -> Result<(), Error> {
    let output = OutputRequest::new(OutputProfile::Hdr10, Precision::Ten);
    let metadata = HdrMetadata {
        mastering_gamut: Gamut::Rec2020,
        max_luminance: 1000.0,
        min_luminance: 0.01,
        max_content_light_level: 800.0,
        max_frame_average_light_level: 400.0,
    };
    let window = Window::new("Title", "org.example.Title")
        .size(400.dp(), 300.dp())
        .min_size(200.dp(), 100.dp())
        .fullscreen(true)
        .background_blur(true)
        .transparent(true)
        .hdr_metadata(metadata)
        .output(output);
    let expected = SurfaceSpec {
        size: Size::new(400.dp(), 300.dp()),
        appearance: Appearance {
            transparent: true,
            output,
            hdr_metadata: Some(metadata),
            background_blur: true,
        },
        role: Role::Window {
            title: "Title".into(),
            app_id: "org.example.Title".into(),
            min_size: Some(Size::new(200.dp(), 100.dp())),
            fullscreen: true,
            draws_decorations: false,
        },
    };
    assert_eq!(window.spec(&mut spacer_view(), &mut NoText)?, expected);
    Ok(())
}
