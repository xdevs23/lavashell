// SPDX-License-Identifier: GPL-3.0-only
use crate::test_renderer::TestPlatformWithoutDrawing;
use crate::{Error, FrameInterval, HostScript};
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::prelude::{Color, Density, DpExt, Easing, Modifier, Tween, Ui, constant};
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{
    Capability, DeviceEvent, KeyAction, KeyboardDevice, PlatformRequest, WatchRequest,
};
use lavashell_runtime::layer_surface::LayerSurface;
use lavashell_runtime::window::Window;
use lavashell_runtime::{App, RunError, SetupError, run, watches};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::time::Duration;

mod paced;
mod window_requests;

const APP_ID: &str = "test.app";
const KEYBOARD: KeyboardDevice = KeyboardDevice(1);
const KEY_A: u32 = 30;
const BLUE: Color = Color::rgb(0x0030_6090);
const SIXTY_HERTZ: FrameInterval = constant!(FrameInterval::per_second(60));

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Run(#[from] RunError<TestPlatformWithoutDrawing>),
    #[error(transparent)]
    Platform(#[from] Error),
    #[error("cannot create a pipe")]
    Pipe(#[from] std::io::Error),
    #[error("cannot watch the pipe")]
    Watch(#[from] watches::Error),
}

type TestResult = Result<(), TestError>;

fn app(compose: impl FnMut(&mut Ui) + 'static) -> Result<App, SetupError> {
    let mut app = App::new().facts(lavashell_test_fixtures::facts());
    app.window(
        Window::new("test", APP_ID).size(100.dp(), 100.dp()),
        compose,
    )?;
    Ok(app)
}

fn key_a() -> DeviceEvent {
    DeviceEvent::Key {
        keyboard: KEYBOARD,
        code: KEY_A,
        stroke: KeyStroke {
            key: None,
            text: Some("a".to_owned()),
        },
        action: KeyAction::Pressed,
    }
}

#[test]
fn an_empty_script_closes_the_windows_and_ends_the_run() -> TestResult {
    let app = app(|_| {})?;
    let (ran, frames) =
        HostScript::new(Density::ONE).serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    assert!(frames?.frames().is_empty());
    Ok(())
}

#[test]
fn a_scripted_key_reaches_the_window_that_has_the_keyboard_focus() -> TestResult {
    let keys = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&keys);
    let app = app(|_| {})?.on_keyboard(move |key| {
        seen.borrow_mut().push((key.code, key.action));
        false
    });
    let script = HostScript::new(Density::ONE)
        .focus_keyboard(KEYBOARD, APP_ID)
        .report(key_a());
    let (ran, _frames_without_a_recording) =
        script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    assert_eq!(*keys.borrow(), [(KEY_A, KeyAction::Pressed)]);
    Ok(())
}

#[test]
fn the_exit_predicate_of_an_app_ends_the_run_before_the_steps_left() -> TestResult {
    let pressed = Rc::new(Cell::new(false));
    let seen = Rc::clone(&pressed);
    let app = (app(|_| {})?)
        .on_keyboard(move |_| {
            seen.set(true);
            false
        })
        .exit_when(move || pressed.get());
    let script = HostScript::new(Density::ONE)
        .focus_keyboard(KEYBOARD, APP_ID)
        .report(key_a())
        .record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    assert!(frames?.frames().is_empty());
    Ok(())
}

#[test]
fn the_exit_time_of_an_app_ends_the_run_on_the_virtual_clock() -> TestResult {
    let app = app(|_| {})?.exit_after(Duration::from_secs(1));
    let script = HostScript::new(Density::ONE)
        .play_until(Duration::from_secs(3600))
        .record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    assert!(frames?.frames().is_empty());
    Ok(())
}

#[test]
fn the_frames_of_a_window_are_found_by_its_app_id() -> TestResult {
    let app = app(|_| {})?;
    let script = HostScript::new(Density::ONE).record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    let frames = frames?;
    assert_eq!(frames.of_window(APP_ID).count(), 1);
    assert_eq!(frames.of_window("other.app").count(), 0);
    Ok(())
}

fn growing_bar() -> Result<App, SetupError> {
    let tween = Tween::new(Duration::from_millis(100))
        .easing(Easing::Linear)
        .preserve_velocity(false);
    app(
        move |ui| match ui.animate_presence("width", 100.dp(), 0.dp(), tween) {
            Ok(width) => ui.spacer(width.get(), 100.dp(), Modifier::new().background(BLUE)),
            Err(error) => ui.fail(error),
        },
    )
}

fn recorded_times(script: HostScript, app: App) -> Result<Vec<Duration>, TestError> {
    let (ran, frames) = script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    Ok((frames?.frames().iter())
        .map(|frame| frame.time.presentation)
        .collect())
}

fn observed_times(
    script: HostScript,
    app: App,
    mut observe: impl FnMut(SurfaceId, &View) + 'static,
) -> Result<Vec<Duration>, TestError> {
    let times = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&times);
    let app = app.view_observer(move |surface, timing, _, view| {
        seen.borrow_mut().push(timing.presentation);
        observe(surface, view);
    });
    let (ran, frames) = script.serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    let _frames_recorded_by_the_script = frames?;
    Ok(times.take())
}

#[test]
fn a_recording_repeats_at_each_interval_until_the_motion_rests() -> TestResult {
    let every = constant!(FrameInterval::per_second(40));
    let script = HostScript::new(Density::ONE).record_until_rest(every);
    assert_eq!(
        recorded_times(script, growing_bar()?)?,
        [0, 25, 50, 75, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn an_app_that_needs_a_missing_capability_fails_before_the_run_starts() {
    let mut app = App::new().facts(lavashell_test_fixtures::facts());
    app.layer(LayerSurface::new("bar"), |_| {});
    let (ran, frames) =
        HostScript::new(Density::ONE).serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    assert!(matches!(
        ran,
        Err(RunError::Unsupported(missing)) if missing == BTreeSet::from([Capability::Panels])
    ));
    assert!(matches!(frames, Err(Error::NotServed(_))));
}

#[test]
fn a_request_that_the_test_platform_cannot_carry_out_fails_the_run() -> TestResult {
    let (reader, _writer) = std::io::pipe()?;
    let mut source = Some(OwnedFd::from(reader));
    let app = app(|_| {})?;
    let actions = app.view_actions();
    let watched = Rc::new(RefCell::new(Ok(())));
    let dispatched = Rc::clone(&watched);
    let app = app.view_observer(move |surface, _, _, _| {
        if let Some(source) = source.take() {
            *dispatched.borrow_mut() = actions.dispatch_when_readable(source, surface, |_| {});
        }
    });
    let (ran, _frames_before_the_failure) =
        HostScript::new(Density::ONE).serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    watched.replace(Ok(()))?;
    assert!(matches!(
        ran,
        Err(RunError::Platform(Error::Unsupported(request)))
            if matches!(*request, PlatformRequest::Watch(WatchRequest::Watch(..)))
    ));
    Ok(())
}
