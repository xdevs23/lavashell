// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::{materials::SurfaceTreatment, prelude::*};
use lavashell_example_support::testing::{
    Shaping, capture_path, click, control, drag, write_first_frames,
};
use lavashell_example_support::{Start, Steps, default_app};
use lavashell_materials_example::{Error, app, model::Demo};
use std::path::{Path, PathBuf};
use std::{cell::Cell, rc::Rc};

type Play = Steps<Demo, PathBuf, Error>;

/// Adds `action` as a step, and writes the frame that shows it to the capture `name`.
fn shot(
    play: &mut Play,
    name: &str,
    action: impl FnOnce(&mut View, Viewport, &Demo) -> Result<(), Error> + 'static,
) {
    play.push(Start::at_rest(), action);
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("materials");
    play.mark(Start::NEXT, capture_path(captures, name));
}

/// Plays `play` on its demo and writes the captures. The window has `size`, or the size measured for the demo when
/// `size` is absent.
fn record(play: Play, demo: &Demo, size: Option<Size>) -> Result<(), Error> {
    let frames = play.play(app(default_app()?, demo, size, false)?, Density::ONE)?;
    Ok(write_first_frames(frames)?)
}

#[test]
fn controls_change_the_material_and_render_offscreen() -> Result<(), Error> {
    let measured = Rc::new(Cell::new(Size::default()));
    let demo = Demo::new(false)?;
    let mut play = Play::new(demo.clone());
    let shaping = Shaping::new()?;
    keyboard_reaches_motion(&mut play, &measured, shaping);
    let window = Rc::clone(&measured);
    play.check(Start::at_rest(), move |view, _, _| {
        let output = control(view, "output-info")?;
        assert!(
            output.y + output.height <= window.get().height - 32.dp(),
            "{output:?}"
        );
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, demo| {
        let area = control(view, "stage")?;
        let p = demo.position.get();
        drag(
            view,
            Point::new(area.x + p.x + 100.dp(), area.y + p.y + 100.dp()),
            Point::new(area.x + p.x + 140.dp(), area.y + p.y + 120.dp()),
        );
        assert!((demo.position.get().x - p.x - 40.dp()).abs() < 0.1.dp());
        click(view, "frosted")?;
        assert_eq!(demo.blur.get(), 18.dp());
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, _| Ok(click(view, "reset")?));
    sliders(&mut play);
    record(play, &demo, None)?;

    let size = measured.get();
    let shorter = Size::new(
        size.width,
        size.height - Viewport::new(size, Density::ONE).pixel(),
    );
    let mut compact = Play::new(demo.clone());
    compact_window(&mut compact, shorter);
    record(compact, &demo, Some(shorter))?;

    let mut offscreen = Play::new(demo.clone());
    shot(&mut offscreen, "offscreen", |_, _, demo| {
        demo.reset();
        demo.hue.set(80.0);
        demo.chroma.set(0.035);
        demo.select(SurfaceTreatment::LiquidGlass);
        Ok(())
    });
    record(offscreen, &demo, None)
}

/// Drags the sliders of the frosted and the paper treatment.
fn sliders(play: &mut Play) {
    for (fraction, name) in [(0.0625, "weak"), (0.75, "strong")] {
        shot(play, &format!("frosted-{name}"), move |view, _, demo| {
            let blur = control(view, "blur")?;
            drag(
                view,
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
            Ok(())
        });
    }
    for key in ["blur", "opacity", "radius", "hue", "chroma"] {
        play.push(Start::at_rest(), move |view, _, _| {
            let r = control(view, key)?;
            drag(
                view,
                Point::new(r.x + 2.dp(), r.y + 10.dp()),
                Point::new(r.x + r.width * 0.7, r.y + 10.dp()),
            );
            Ok(())
        });
    }
    play.check(Start::at_rest(), |_, _, demo| {
        assert!(demo.hue.get() > 200.0);
        assert!(demo.blur.get() > 20.dp());
        Ok(())
    });

    play.push(Start::at_rest(), |view, _, _| Ok(click(view, "paper")?));
    play.push(Start::at_rest(), |view, _, demo| {
        let blur = demo.blur.get();
        let refraction = demo.refraction.get();
        for key in ["blur", "refraction"] {
            let r = control(view, key)?;
            drag(
                view,
                Point::new(r.x + 2.dp(), r.y + 10.dp()),
                Point::new(r.x + r.width * 0.9, r.y + 10.dp()),
            );
        }
        assert_eq!(demo.blur.get(), blur);
        assert_eq!(demo.refraction.get(), refraction);
        Ok(())
    });
}

/// Checks the demo, in the state left by the steps before, in a window of size `shorter`, one device pixel shorter
/// than the measured height. At that height the controls move closer together. The stage and the output information
/// have to fit the window, and the panel has to follow a drag in it.
fn compact_window(play: &mut Play, shorter: Size) {
    play.check(Start::at_rest(), move |view, _, _| {
        for key in ["stage", "output-info"] {
            let bounds = control(view, key)?;
            assert!(
                bounds.y + bounds.height <= shorter.height - 32.dp(),
                "{key}: {bounds:?}"
            );
        }
        Ok(())
    });
    play.push(Start::at_rest(), |_, _, demo| {
        demo.reset();
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, demo| {
        let area = control(view, "stage")?;
        let panel = demo.panel_position(Size::new(area.width, area.height));
        drag(
            view,
            Point::new(area.x + panel.x + 15.dp(), area.y + panel.y + 5.dp()),
            Point::new(area.x + panel.x + 40.dp(), area.y + panel.y + 5.dp()),
        );
        assert!((demo.position.get().x - panel.x - 25.dp()).abs() < 0.1.dp());
        Ok(())
    });
}

/// Checks that keyboard focus paints its ring around the motion button and that Enter presses it. Stores the size of
/// the window, measured for the demo, in `measured`.
fn keyboard_reaches_motion(play: &mut Play, measured: &Rc<Cell<Size>>, shaping: Shaping) {
    let measuring = Rc::clone(measured);
    play.push(Start::at_rest(), move |view, viewport, demo| {
        measuring.set(viewport.size);
        for _ in 0..2 {
            view.input(InputEvent::Key {
                key: Key::Tab,
                shift: false,
            });
        }
        let tree = shaping.scene(view, viewport)?;
        let motion = control(view, "motion")?;
        let now = view.motion().now();
        let visible_focus = tree.at(now).leaf_commands().any(|command| {
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
    });
}
