// SPDX-License-Identifier: GPL-3.0-only
use crate::*;
use icu_locale_core::LanguageIdentifier;
use icu_locale_core::subtags::language;
use lavashell_core::accessibility::AccessibleNode;
use lavashell_core::prelude::*;
use lavashell_test_fixtures::{
    DOUBLE_TAP, FrameFailure, LONG_PRESS, REPEAT_DELAY, REPEAT_INTERVAL, WithoutFailures,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("a frame of the view failed")]
    Failed(#[from] FrameFailure),
    #[error("an animation cannot take a value")]
    Animation(#[from] AnimationError),
    #[error("a layout of the test does not fit the keys")]
    Layout(#[from] LayoutError),
    #[error("a length used by the test is not valid")]
    Number(#[from] NumberError),
    #[error("the view exports no {role:?} named {name}")]
    NoNode { role: Role, name: String },
    #[error("\"{0}\" is not composed")]
    NotComposed(String),
    #[error("{0} is not a density")]
    Density(f64),
    #[error("the host has no session")]
    NoSession,
    #[error("the test moved the clock before its start")]
    BeforeStart,
    #[error("the layout offers fewer than {count} further letters for {letter}")]
    FewFurtherLetters { letter: char, count: usize },
}

mod accessibility;
mod dock;
mod host;
mod host_view;
mod layouts;
mod look;
mod painted;
mod popups;
mod settings;
mod shift;
mod stages;
mod timelines;
mod transform;
mod typing;

use host_view::Host;
use painted::{backdrops, fused_rows, leaves, painted, rings, texts, unions};

static BLANK: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[],
};

const ICONS: KeyboardIcons = KeyboardIcons {
    search: &BLANK,
    done: &BLANK,
    next: &BLANK,
    go: &BLANK,
    send: &BLANK,
    enter: &BLANK,
    shift: &BLANK,
    caps_lock: &BLANK,
    delete: &BLANK,
    globe: &BLANK,
    hide: &BLANK,
    active_layout: &BLANK,
};

fn words() -> KeyboardText {
    let word = |word: &str| word.to_owned();
    KeyboardText {
        keyboard: word("Keyboard"),
        layouts: word("Keyboard layouts"),
        more_letters: word("More letters"),
        search: word("Search"),
        done: word("Done"),
        next: word("Next"),
        go: word("Go"),
        send: word("Send"),
        enter: word("Enter"),
        symbols_label: word("?123"),
        letters_label: word("ABC"),
        more_symbols_label: word("=\\<"),
        symbols: word("Symbols"),
        letters: word("Letters"),
        more_symbols: word("More symbols"),
        shift: word("Shift"),
        delete: word("Delete"),
        globe: word("Switch layout"),
        hide: word("Hide keyboard"),
        space: word("Space"),
    }
}

const PANEL: NonNegativeDp = constant!(NonNegativeDp::try_new(Dp::new(350.0)));

const KEY_HEIGHT: Dp = Dp::new(76.0);

fn shipped() -> Result<&'static ShippedLayouts, TestError> {
    Ok(ShippedLayouts::load()?)
}

fn settings() -> Result<KeyboardSettings, TestError> {
    Ok(KeyboardSettings {
        layouts: vec![&shipped()?.english],
        text: words(),
        icons: ICONS,
        panel_height: PANEL,
    })
}

fn offering(layouts: Vec<&'static Layout>) -> Result<(KeyboardSettings, PlatformFacts), TestError> {
    let locales = layouts
        .iter()
        .map(|layout| LanguageIdentifier::from(layout.language()));
    let facts = PlatformFacts {
        locales: locales.collect(),
        ..lavashell_test_fixtures::facts()
    };
    Ok((
        KeyboardSettings {
            layouts,
            ..settings()?
        },
        facts,
    ))
}

struct HalfEmPerByteMetrics;

impl TextMeasurer for HalfEmPerByteMetrics {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        Size::new(
            (style.size * text.bytes().map(|_| 0.5_f64).sum::<f64>()).min(width),
            style.size * 1.3,
        )
    }
}

fn touch(point: i32) -> TouchId {
    TouchId { seat: 0, point }
}

fn finger(point: i32, at: Point) -> TouchSample {
    TouchSample::finger(touch(point), at)
}

fn near(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|difference| difference.abs() < 0.5.dp())
}

fn near_length(length: Dp, expected: Dp) -> bool {
    (length - expected).abs() < 0.05.dp()
}

const SCREEN: Size = Size::new(Dp::new(1037.0), Dp::new(466.0));

#[derive(Default)]
struct Scene {
    builder: SceneBuilder,
    now: Duration,
}

fn just_before(time: Duration) -> Result<Duration, TestError> {
    time.checked_sub(Duration::from_millis(1))
        .ok_or(TestError::BeforeStart)
}

const FRAME: Duration = Duration::from_millis(16);

fn millis(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

fn frame_at(view: &mut View, scene: &mut Scene, time: Duration) -> Result<(), TestError> {
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..FrameTime::default()
    };
    let viewport = Viewport::new(SCREEN, Density::ONE);
    (view.frame_into_at(
        viewport,
        &mut HalfEmPerByteMetrics,
        timing,
        &mut scene.builder,
    ))
    .without_failures()?;
    scene.now = time;
    Ok(())
}

fn refresh(view: &mut View, scene: &mut Scene) -> Result<(), TestError> {
    let now = scene.now;
    frame_at(view, scene, now)
}

fn settle(view: &mut View, scene: &mut Scene) -> Result<(), TestError> {
    refresh(view, scene)?;
    while view.motion().is_active() {
        let Some(deadline) = view
            .next_deadline()
            .filter(|deadline| *deadline > scene.now)
        else {
            break;
        };
        frame_at(view, scene, deadline)?;
    }
    Ok(())
}

fn run_until(
    view: &mut View,
    scene: &mut Scene,
    mut done: impl FnMut(&View, &Scene) -> bool,
) -> Result<(), TestError> {
    while !done(view, scene) {
        assert!(
            view.motion().is_active() || view.next_deadline().is_some(),
            "nothing is left that could change the view"
        );
        let time = scene.now + FRAME;
        frame_at(view, scene, time)?;
    }
    Ok(())
}

fn node<'a>(view: &'a View, role: &Role, name: &str) -> Result<&'a AccessibleNode, TestError> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
        .ok_or_else(|| TestError::NoNode {
            role: role.clone(),
            name: name.to_owned(),
        })
}

fn composed_bounds(view: &View, tag: &'static str) -> Result<Rect, TestError> {
    view.bounds_of(&tag)
        .ok_or_else(|| TestError::NotComposed(tag.to_owned()))
}

fn focused(view: &View, tag: &'static str) -> bool {
    view.interaction_state(&tag)
        .is_some_and(|state| state.focus.focused)
}

fn has(view: &View, role: &Role, name: &str) -> bool {
    (view.accessibility().nodes.values())
        .any(|node| node.role == *role && node.name.as_deref() == Some(name))
}

fn key_bounds(view: &View, name: &str) -> Result<Rect, TestError> {
    Ok(node(view, &Role::Button, name)?.bounds)
}

fn key(view: &View, name: &str) -> Result<Point, TestError> {
    Ok(key_bounds(view, name)?.center())
}

fn tap(view: &mut View, name: &str) -> Result<(), TestError> {
    let at = key(view, name)?;
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
    Ok(())
}

fn session(id: SessionId, hints: TextHints, text: FieldText) -> TextSession {
    TextSession {
        id,
        presses: FieldPresses(0),
        hints,
        multiline: false,
        text,
    }
}

fn above_keyboard(ui: &mut Ui, content: impl FnOnce(&mut Ui), keyboard: impl FnOnce(&mut Ui)) {
    ui.layout(
        Axis::Vertical,
        Modifier::new().fill_max_width().fill_max_height(),
        |ui| {
            ui.layout(
                Arrangement::Overlay,
                Modifier::new().weight(1.0).tag("content"),
                content,
            );
            keyboard(ui);
        },
    );
}

fn ends_at_glass(view: &View, scene: &Scene) -> bool {
    let glass = backdrops(scene)[0];
    view.bounds_of(&"content")
        .is_some_and(|content| (content.y + content.height - glass.y).abs() < 0.01.dp())
}

trait Clocked {
    fn clock(&mut self) -> (&mut View, &mut Scene, &mut Duration);

    fn settle(&mut self) -> Result<(), TestError> {
        let (view, scene, time) = self.clock();
        settle(view, scene)?;
        *time = (*time).max(scene.now);
        Ok(())
    }

    fn at(&mut self, time: Duration) -> Result<(), TestError> {
        let (view, scene, current) = self.clock();
        *current = time;
        frame_at(view, scene, time)
    }

    fn frame(&mut self) -> Result<(), TestError> {
        let (view, scene, time) = self.clock();
        frame_at(view, scene, *time)
    }
}

struct Fixture {
    view: View,
    scene: Scene,
    text: State<String>,
    secret: ProtectedText,
    keyboard: Keyboard,
    time: Duration,
}

impl Fixture {
    fn new(hints: TextHints) -> Result<Self, TestError> {
        Self::with(hints, settings()?)
    }

    fn with(hints: TextHints, settings: KeyboardSettings) -> Result<Self, TestError> {
        let text = State::new(String::new());
        let (settings, layout) = (State::new(settings), State::new(&shipped()?.english));
        let placement = State::new(Placement::FullWidth);
        let keyboard = Keyboard::new(Target::View, &settings, layout, placement)?;
        let secret = ProtectedText::new();
        let (value, composed) = (text.clone(), keyboard.clone());
        let protected = secret.clone();
        let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
            let value = value.clone();
            let fields = |ui: &mut Ui| {
                ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                    let options = TextFieldOptions {
                        hints,
                        ..TextFieldOptions::default()
                    };
                    let field = Modifier::new().tag("field").size(400.dp(), 40.dp());
                    ui.text_input(value.get(), options, field, move |text| value.set(text));
                    ui.text_input(
                        "",
                        TextFieldOptions::default(),
                        Modifier::new().tag("second").size(400.dp(), 40.dp()),
                        |_| {},
                    );
                    ui.protected_input(
                        protected.clone(),
                        TextFieldOptions::default(),
                        Modifier::new().tag("secret").size(400.dp(), 40.dp()),
                    );
                });
            };
            above_keyboard(ui, fields, |ui| {
                ui.keyboard(&composed, Modifier::new());
            });
        });
        let mut scene = Scene::default();
        frame_at(&mut view, &mut scene, Duration::ZERO)?;
        Ok(Self {
            view,
            scene,
            text,
            secret,
            keyboard,
            time: Duration::ZERO,
        })
    }

    fn focus(&mut self, tag: &'static str) -> Result<(), TestError> {
        let at = composed_bounds(&self.view, tag)?.center();
        self.view.input(InputEvent::Press(at));
        self.view.input(InputEvent::Release(at));
        self.settle()
    }

    fn type_keys(&mut self, names: &[&str]) -> Result<(), TestError> {
        for name in names {
            tap(&mut self.view, name)?;
            self.frame()?;
        }
        Ok(())
    }

    fn touch_down(&mut self, point: i32, name: &str) -> Result<(), TestError> {
        let at = key(&self.view, name)?;
        self.view.input(InputEvent::TouchDown(finger(point, at)));
        Ok(())
    }

    fn touch_up(&mut self, point: i32, name: &str) -> Result<(), TestError> {
        let at = key(&self.view, name)?;
        self.view.input(InputEvent::TouchUp(finger(point, at)));
        Ok(())
    }
}

impl Clocked for Fixture {
    fn clock(&mut self) -> (&mut View, &mut Scene, &mut Duration) {
        (&mut self.view, &mut self.scene, &mut self.time)
    }
}
