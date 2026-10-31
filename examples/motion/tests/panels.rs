// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
use lavashell_example_support::{
    Capture, Error as ExampleError,
    testing::{click, control, drag, node, without_failures},
};
use lavashell_motion::{
    CopyKey, Error, copy,
    model::Demo,
    ui::{self, WINDOW},
};
use std::{path::Path, time::Duration};

/// The view of the demo, framed at times set by the test. The demo is stepped before each frame, as the
/// host does.
struct Harness {
    view: View,
    demo: Demo,
    capture: Capture,
    tree: SceneTree,
    time: Duration,
}

impl Harness {
    fn new(demo: Demo) -> Result<Self, Error> {
        let state = demo.clone();
        let view = View::new(lavashell_example_support::platform(), move |ui| {
            ui::compose(ui, &state);
        });
        Ok(Self {
            view,
            demo,
            capture: Capture::new()?,
            tree: SceneTree::default(),
            time: Duration::ZERO,
        })
    }

    fn frame(&mut self) -> Result<(), Error> {
        let timing = FrameTime {
            now: self.time,
            presentation: self.time,
            ..FrameTime::default()
        };
        self.demo.step(timing).map_err(Error::Motion)?;
        let viewport = Viewport::new(WINDOW, Density::ONE);
        let shaper = self.capture.painter.shaper();
        let shown = (self.view).show_frame_at(&mut self.tree, viewport, shaper, timing);
        without_failures(shown).map_err(Error::Example)
    }

    /// Frames the view `delay` after the last frame.
    fn wait(&mut self, delay: Duration) -> Result<(), Error> {
        self.time += delay;
        self.frame()
    }

    /// Frames the view at its next deadline.
    fn advance(&mut self) -> Result<(), Error> {
        self.time = (self.view.next_deadline())
            .filter(|deadline| *deadline > self.time)
            .ok_or(ExampleError::NothingMoves)?;
        self.frame()
    }

    fn save(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        let viewport = Viewport::new(WINDOW, Density::ONE);
        Ok(self.capture.save(&mut self.view, viewport, path)?)
    }

    fn click(&mut self, tag: &str) -> Result<(), Error> {
        Ok(click(&mut self.view, tag)?)
    }

    fn control(&self, tag: &str) -> Result<Rect, Error> {
        Ok(control(&self.view, tag)?)
    }

    fn node(&self, role: &Role, name: &str) -> Result<AccessibleNode, Error> {
        Ok(node(&self.view, role, name)?.clone())
    }
}

#[test]
fn panels_retarget_pause_seek_and_settle_with_reduced_motion() -> Result<(), Error> {
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("motion");
    let mut harness = Harness::new(Demo::new(true)?)?;
    harness.frame()?;
    harness.save(captures.join("0.png"))?;
    let mut rest = 0;
    while !(harness.demo.spring.is_finished() && harness.demo.morph_finished()) {
        harness.advance()?;
        rest += 1;
        harness.save(captures.join(format!("{rest}.png")))?;
    }

    assert!(harness.view.bounds_of("Undertow").is_some());
    harness.click(copy(CopyKey::AutoPlay))?;
    assert!(!harness.demo.autoplay.get());
    harness.frame()?;
    harness.click(copy(CopyKey::Collapse))?;
    harness.frame()?;
    harness.wait(Duration::from_millis(100))?;
    let before = harness.control("Undertow")?;
    assert!(before.width > 212.dp() && before.width < 478.dp());
    harness.click("Undertow")?;
    harness.frame()?;
    assert!(
        (harness.control("Undertow")?)
            .vector()
            .sub(&before.vector())
            .max_abs()
            < 0.001
    );
    assert!(harness.demo.expanded.get());

    pause_seek_and_play(&mut harness)?;
    let timeline = harness.node(&Role::Slider, copy(CopyKey::TimelineTitle))?;
    timeline_semantics(&mut harness, &timeline);
    morph_and_spring(&mut harness)?;
    harness.click(copy(CopyKey::ReducedMotion))?;
    harness.frame()?;
    assert!(!harness.view.motion().is_active());
    assert_eq!(
        harness.demo.spring.get().to_bits(),
        harness.demo.target.get().to_bits()
    );
    harness.save(captures.join("interactions.png"))?;

    harness.demo.set_reduced(false).map_err(Error::Motion)?;
    harness.demo.restart().map_err(Error::Motion)?;
    harness.view.provide(&REDUCED_MOTION, true);
    harness.wait(Duration::from_millis(100))?;
    assert!(
        !harness.view.motion().is_active(),
        "the example must honor the effective reduced-motion preference"
    );
    let reduced = harness.node(&Role::Switch, copy(CopyKey::ReducedMotion))?;
    assert_eq!(reduced.state.semantics.checked, Some(Checked::True));
    assert!(
        reduced.state.semantics.disabled,
        "a local demo toggle cannot undo an enforced accessibility preference"
    );
    Ok(())
}

fn pause_seek_and_play(harness: &mut Harness) -> Result<(), Error> {
    harness.click(copy(CopyKey::Pause))?;
    let held = harness.demo.timeline.get();
    harness.wait(Duration::from_millis(300))?;
    assert_eq!(harness.demo.timeline.get().to_bits(), held.to_bits());
    let at = harness.control("timeline-seek")?.center();
    drag(&mut harness.view, at, at);
    let sought = harness.demo.timeline.get();
    assert!(sought > 0.45 && sought < 0.6);
    harness.wait(Duration::from_millis(200))?;
    assert_eq!(harness.demo.timeline.get().to_bits(), sought.to_bits());
    harness.click(copy(CopyKey::Play))?;
    harness.wait(Duration::from_millis(100))?;
    assert!(harness.demo.timeline.get() > sought);

    let compositions = |view: &View| {
        (
            view.stats().root_compositions,
            view.stats().component_compositions,
        )
    };
    let composed = compositions(&harness.view);
    harness.wait(Duration::from_millis(10))?;
    harness.wait(Duration::from_millis(10))?;
    assert_eq!(
        compositions(&harness.view),
        composed,
        "animated semantics must preserve paint-only motion"
    );
    Ok(())
}

fn timeline_semantics(harness: &mut Harness, timeline: &AccessibleNode) {
    let length = harness.demo.timeline_length();
    let playback_progress = harness.demo.timeline.elapsed().as_secs_f64() / length.as_secs_f64();
    assert!((0.0..=1.0).contains(&playback_progress));
    assert!(
        matches!(timeline.value, Some(AccessibleValue::Number(value))
            if (value - playback_progress).abs() <= f64::EPSILON),
        "semantic progress must match the playback position shown by the slider"
    );
    let elapsed = harness.demo.timeline.elapsed();
    assert_eq!(
        harness.view.accessibility_action(ActionRequest {
            target: timeline.id,
            action: AccessibilityAction::Focus
        }),
        Ok(ActionOutcome::Performed)
    );
    harness.view.input(InputEvent::Key {
        key: Key::Right,
        shift: false,
    });
    let stepped = harness.demo.timeline.elapsed();
    assert!(
        stepped > elapsed,
        "keyboard seek must start from current animated progress"
    );
    assert_eq!(
        harness.view.accessibility_action(ActionRequest {
            target: timeline.id,
            action: AccessibilityAction::Increment,
        }),
        Ok(ActionOutcome::Performed)
    );
    assert!(harness.demo.timeline.elapsed() > stepped);
}

fn morph_and_spring(harness: &mut Harness) -> Result<(), Error> {
    let shape = harness.demo.shape();
    assert!(shape.is_some());
    harness.click(copy(CopyKey::Morph))?;
    assert_eq!(harness.demo.shape(), shape);
    harness.wait(Duration::from_millis(180))?;
    assert_ne!(harness.demo.shape(), shape);
    let shape = harness.demo.shape();
    harness.click(copy(CopyKey::Morph))?;
    assert_eq!(harness.demo.shape(), shape);

    let slider = harness.control("spring-target")?;
    let at = Point::new(slider.x + slider.width * 0.75, slider.center().y);
    let position = harness.demo.spring.get();
    drag(&mut harness.view, at, at);
    assert!((harness.demo.target.get() - 0.75).abs() < 0.01);
    assert_eq!(harness.demo.spring.get().to_bits(), position.to_bits());
    harness.wait(Duration::from_millis(200))?;
    assert_ne!(harness.demo.spring.get().to_bits(), position.to_bits());
    Ok(())
}
