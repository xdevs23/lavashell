// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
use lavashell_example_support::{
    Capture, Error as ExampleError, ThemeChoice,
    testing::{click, control, drag, node, scene},
    without_failures,
};
use lavashell_motion::{
    CopyKey, Error, Tag, copy,
    model::Demo,
    ui::{self, WINDOW},
};
use std::{path::Path, time::Duration};

struct Harness {
    view: View,
    demo: Demo,
    capture: Capture,
    builder: SceneBuilder,
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
            builder: SceneBuilder::default(),
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
        let shown = (self.view).frame_into_at(viewport, shaper, timing, &mut self.builder);
        without_failures(shown).map_err(Error::Example)
    }

    fn wait(&mut self, delay: Duration) -> Result<(), Error> {
        self.time += delay;
        self.frame()
    }

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

    fn choose(&mut self, theme: ThemeChoice) -> Result<(), Error> {
        click(&mut self.view, theme)?;
        self.frame()
    }

    fn page_fill(&mut self) -> Result<Option<DrawCommand>, Error> {
        let viewport = Viewport::new(WINDOW, Density::ONE);
        let shaper = self.capture.painter.shaper();
        let tree = scene(&mut self.view, viewport, shaper)?;
        Ok(tree.at(Duration::ZERO).leaf_commands().next().cloned())
    }

    fn click(&mut self, tag: Tag) -> Result<(), Error> {
        Ok(click(&mut self.view, tag)?)
    }

    fn control(&self, tag: Tag) -> Result<Rect, Error> {
        Ok(control(&self.view, tag)?)
    }

    fn node(&self, role: &Role, name: &str) -> Result<AccessibleNode, Error> {
        Ok(node(&self.view, role, name)?.clone())
    }
}

#[test]
fn panels_retarget_pause_seek_and_settle_with_reduced_motion() -> Result<(), Error> {
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("motion");
    let mut harness = Harness::new(Demo::new(true, ThemeChoice::System)?)?;
    harness.frame()?;
    harness.save(captures.join("0.png"))?;
    let mut rest = 0;
    while !(harness.demo.spring.is_finished() && harness.demo.morph_finished()) {
        harness.advance()?;
        rest += 1;
        harness.save(captures.join(format!("{rest}.png")))?;
    }

    let expanded = harness.control(Tag::Card)?;
    harness.click(Tag::Control(CopyKey::AutoPlay))?;
    assert!(!harness.demo.autoplay.get());
    harness.frame()?;
    harness.click(Tag::Control(CopyKey::Collapse))?;
    harness.frame()?;
    harness.wait(Duration::from_millis(100))?;
    let before = harness.control(Tag::Card)?;
    assert!(
        before.width < expanded.width && harness.view.motion().is_active(),
        "the card is still collapsing"
    );
    harness.click(Tag::Card)?;
    harness.frame()?;
    assert!(
        (harness.control(Tag::Card)?)
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
    harness.click(Tag::Control(CopyKey::ReducedMotion))?;
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
    harness.click(Tag::Control(CopyKey::Pause))?;
    let held = harness.demo.timeline.get();
    harness.wait(Duration::from_millis(300))?;
    assert_eq!(harness.demo.timeline.get().to_bits(), held.to_bits());
    let at = harness.control(Tag::TimelineSeek)?.center();
    drag(&mut harness.view, at, at);
    let sought = harness.demo.timeline.get();
    assert!(sought > 0.45 && sought < 0.6);
    harness.wait(Duration::from_millis(200))?;
    assert_eq!(harness.demo.timeline.get().to_bits(), sought.to_bits());
    harness.click(Tag::Control(CopyKey::Play))?;
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
    harness.click(Tag::Control(CopyKey::Morph))?;
    assert_eq!(harness.demo.shape(), shape);
    harness.wait(Duration::from_millis(180))?;
    assert_ne!(harness.demo.shape(), shape);
    let shape = harness.demo.shape();
    harness.click(Tag::Control(CopyKey::Morph))?;
    assert_eq!(harness.demo.shape(), shape);

    let slider = harness.control(Tag::SpringTarget)?;
    let at = Point::new(slider.x + slider.width * 0.75, slider.center().y);
    let position = harness.demo.spring.get();
    drag(&mut harness.view, at, at);
    assert!((harness.demo.target.get() - 0.75).abs() < 0.01);
    assert_eq!(harness.demo.spring.get().to_bits(), position.to_bits());
    harness.wait(Duration::from_millis(200))?;
    assert_ne!(harness.demo.spring.get().to_bits(), position.to_bits());
    Ok(())
}

#[test]
fn the_theme_switch_repaints_the_window() -> Result<(), Error> {
    let mut harness = Harness::new(Demo::new(false, ThemeChoice::System)?)?;
    harness.frame()?;
    let by_system = harness.page_fill()?;
    harness.choose(ThemeChoice::Light)?;
    assert_ne!(harness.page_fill()?, by_system);
    harness.choose(ThemeChoice::Dark)?;
    assert_eq!(harness.page_fill()?, by_system);
    Ok(())
}
