// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::{materials::SurfaceTreatment, prelude::*};
use lavashell_example_support::{
    Capture,
    testing::{click, control, drag, scene, without_failures},
};
use lavashell_materials_example::{Error, model::Demo, ui};
use std::{path::Path, time::Duration};

fn show(
    view: &mut View,
    tree: &mut SceneTree,
    viewport: Viewport,
    capture: &mut Capture,
) -> Result<(), Error> {
    without_failures(view.show_frame(tree, viewport, &mut capture.painter)).map_err(Error::Example)
}

#[test]
fn controls_change_the_material_and_render_offscreen() -> Result<(), Error> {
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("materials");
    let demo = Demo::new(false)?;
    let composed = demo.clone();
    let mut view = View::new(lavashell_example_support::platform(), move |u| {
        ui::compose(u, &composed);
    });
    let mut capture = Capture::new()?;
    let mut shown = SceneTree::default();
    let size = capture.preferred_size(&mut view)?;
    let window = Viewport::new(size, Density::ONE);
    show(&mut view, &mut shown, window, &mut capture)?;

    keyboard_reaches_motion(&mut view, &demo, &mut capture, window)?;
    show(&mut view, &mut shown, window, &mut capture)?;
    let output = control(&view, "output-info")?;
    assert!(
        output.y + output.height <= size.height - 32.dp(),
        "{output:?}"
    );

    let area = control(&view, "stage")?;
    let p = demo.position.get();
    drag(
        &mut view,
        Point::new(area.x + p.x + 100.dp(), area.y + p.y + 100.dp()),
        Point::new(area.x + p.x + 140.dp(), area.y + p.y + 120.dp()),
    );
    assert!((demo.position.get().x - p.x - 40.dp()).abs() < 0.1.dp());
    click(&mut view, "frosted")?;
    assert_eq!(demo.blur.get(), 18.dp());
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "reset")?;
    show(&mut view, &mut shown, window, &mut capture)?;

    let blur = control(&view, "blur")?;
    for (fraction, name) in [(0.0625, "weak"), (0.75, "strong")] {
        drag(
            &mut view,
            Point::new(blur.x + 2.dp(), blur.y + 10.dp()),
            Point::new(blur.x + blur.width * fraction, blur.y + 10.dp()),
        );
        let sigma = 32.dp() * fraction;
        assert!((demo.blur.get() - sigma).abs() < 0.01.dp());
        let blurred = |backdrop: Backdrop| (backdrop.blur.get() - sigma).abs() < 0.01.dp();
        assert!(
            demo.material()
                .is_ok_and(|material| material.backdrop.is_some_and(blurred)),
            "the material blurs what is behind it by {sigma:?}"
        );
        capture.save(
            &mut view,
            window,
            captures.join(format!("frosted-{name}.png")),
        )?;
    }
    for key in ["blur", "opacity", "radius", "hue", "chroma"] {
        let r = control(&view, key)?;
        drag(
            &mut view,
            Point::new(r.x + 2.dp(), r.y + 10.dp()),
            Point::new(r.x + r.width * 0.7, r.y + 10.dp()),
        );
        show(&mut view, &mut shown, window, &mut capture)?;
    }
    assert!(demo.hue.get() > 200.0);
    assert!(demo.blur.get() > 20.dp());

    click(&mut view, "paper")?;
    show(&mut view, &mut shown, window, &mut capture)?;
    let blur = demo.blur.get();
    let refraction = demo.refraction.get();
    for key in ["blur", "refraction"] {
        let r = control(&view, key)?;
        drag(
            &mut view,
            Point::new(r.x + 2.dp(), r.y + 10.dp()),
            Point::new(r.x + r.width * 0.9, r.y + 10.dp()),
        );
    }
    assert_eq!(demo.blur.get(), blur);
    assert_eq!(demo.refraction.get(), refraction);

    compact_window(&mut view, &mut shown, &demo, &mut capture, window)?;

    demo.reset();
    demo.hue.set(80.0);
    demo.chroma.set(0.035);
    demo.select(SurfaceTreatment::LiquidGlass);
    Ok(capture.save(&mut view, window, captures.join("offscreen.png"))?)
}

/// Checks the demo in a window one device pixel shorter than the height measured by the demo, where the
/// controls move closer together. The stage and the output information have to fit that window, and the panel
/// has to follow a drag in it.
fn compact_window(
    view: &mut View,
    shown: &mut SceneTree,
    demo: &Demo,
    capture: &mut Capture,
    window: Viewport,
) -> Result<(), Error> {
    let shorter = Size::new(window.size.width, window.size.height - window.pixel());
    let compact = Viewport::new(shorter, window.density);
    // The first frame measures the shorter window. The second frame lays out the demo for that measurement.
    show(view, shown, compact, capture)?;
    show(view, shown, compact, capture)?;
    for key in ["stage", "output-info"] {
        let bounds = control(view, key)?;
        assert!(
            bounds.y + bounds.height <= shorter.height - 32.dp(),
            "{key}: {bounds:?}"
        );
    }
    demo.reset();
    show(view, shown, compact, capture)?;
    let area = control(view, "stage")?;
    let panel = demo.panel_position(Size::new(area.width, area.height));
    drag(
        view,
        Point::new(area.x + panel.x + 15.dp(), area.y + panel.y + 5.dp()),
        Point::new(area.x + panel.x + 40.dp(), area.y + panel.y + 5.dp()),
    );
    assert!((demo.position.get().x - panel.x - 25.dp()).abs() < 0.1.dp());
    Ok(())
}

/// Checks that keyboard focus paints its ring around the motion button and that Enter presses it.
fn keyboard_reaches_motion(
    view: &mut View,
    demo: &Demo,
    capture: &mut Capture,
    window: Viewport,
) -> Result<(), Error> {
    for _ in 0..2 {
        view.input(InputEvent::Key {
            key: Key::Tab,
            shift: false,
        });
    }
    let tree = scene(view, window, &mut capture.painter)?;
    let motion = control(view, "motion")?;
    let visible_focus = tree.at(Duration::ZERO).leaf_commands().any(|command| {
        matches!(command, DrawCommand::Stroke { shape, stroke }
            if shape.bounds() == motion && stroke.width.get() == 2.dp())
    });
    assert!(
        visible_focus,
        "keyboard focus must paint around the motion button"
    );
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert!(
        demo.animate.get(),
        "keyboard activation must reach the motion callback"
    );
    demo.animate.set(false);
    Ok(())
}
