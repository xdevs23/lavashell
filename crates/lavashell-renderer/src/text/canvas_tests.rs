// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_test_fixtures::WithoutFailures;
use std::{cell::Cell, rc::Rc};

#[test]
fn canvas_only_text_observes_policy_in_root_and_retained_paint_scopes() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let counts = [Rc::new(Cell::new(0_u32)), Rc::new(Cell::new(0_u32))];
    let observers = counts.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let count = observers[0].clone();
            ui.canvas(
                Size::new(400.dp(), 60.dp()),
                Modifier::new().tag("root canvas"),
                move |drawing| {
                    count.set(count.get() + 1);
                    drawing.text(
                        "Mon 5 Oct e\u{301} office",
                        Rect::new(5.dp(), 4.dp(), 390.dp(), 40.dp()),
                        TextStyle::new("probe", 13.sp(), Color::WHITE),
                    );
                },
            );
            let retained_count = observers[1].clone();
            ui.scoped("canvas packet", Modifier::new(), move |ui| {
                let count = retained_count.clone();
                ui.canvas(
                    Size::new(400.dp(), 60.dp()),
                    Modifier::new().tag("retained canvas"),
                    move |drawing| {
                        count.set(count.get() + 1);
                        drawing.text(
                            "Mon 5 Oct e\u{301} office",
                            Rect::new(5.dp(), 4.dp(), 390.dp(), 40.dp()),
                            TextStyle::new("probe", 13.sp(), Color::WHITE),
                        );
                    },
                );
            });
        });
    });
    let target = TextDestination {
        opaque: true,
        geometry: PixelGeometry::Rgb,
    };
    let size = Size::new(600.dp(), 160.dp());
    painter.set_text_rendering(TextRendering::default(), target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    (view.show_frame(&mut tree, viewport, &mut painter)).without_failures()?;
    view.update_accessibility();
    let bounds = [
        view.bounds_of("root canvas"),
        view.bounds_of("retained canvas"),
    ];
    let compositions = view.stats().root_compositions;
    let painted_before = view.stats().painted_components;
    let old_counts = counts.each_ref().map(|count| count.get());
    assert!(!view.needs_frame());
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        },
        target,
    );
    assert!(
        view.needs_frame(),
        "canvas-only View did not observe renderer policy"
    );
    (view.show_frame(&mut tree, viewport, &mut painter)).without_failures()?;
    view.update_accessibility();
    assert_eq!(
        [
            view.bounds_of("root canvas"),
            view.bounds_of("retained canvas")
        ],
        bounds
    );
    assert_eq!(view.stats().root_compositions, compositions);
    assert!(view.stats().painted_components > painted_before);
    for (count, old) in counts.iter().zip(old_counts) {
        assert!(
            count.get() > old,
            "canvas callback remained retained after policy change"
        );
    }
    assert!(!view.needs_frame());
    painter.set_text_rendering(TextRendering::default(), target);
    assert!(view.needs_frame());
    (view.show_frame(&mut tree, viewport, &mut painter)).without_failures()?;
    view.update_accessibility();
    assert!(!view.needs_frame());
    Ok(())
}
