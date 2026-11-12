// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, FrameInterval, HostScript, RecordedFrame, TestPlatform};
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::prelude::{Color, Density, DpExt, Easing, Modifier, Tween, Ui, constant};
use lavashell_platform::{
    Capability, DeviceEvent, KeyAction, KeyboardDevice, Platform, PlatformRequest, WatchRequest,
};
use lavashell_renderer::skia::{
    self, AlphaType, ColorSpace, ColorType, Data, ISize, Image, ImageInfo,
};
use lavashell_runtime::window::Window;
use lavashell_runtime::{App, RunError, SetupError, run, watches};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::time::Duration;

mod paced;

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
    Run(#[from] RunError<TestPlatform>),
    #[error(transparent)]
    Platform(#[from] Error),
    #[error("cannot create a pipe")]
    Pipe(#[from] std::io::Error),
    #[error("cannot watch the pipe")]
    Watch(#[from] watches::Error),
}

type TestResult = Result<(), TestError>;

/// An app with one window of 100 by 100 dp, whose view `compose` composes.
fn app(compose: impl FnMut(&mut Ui) + 'static) -> Result<App, SetupError> {
    let mut app = App::new(lavashell_test_fixtures::facts());
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

/// The size of `frame` and its pixel in the middle, as 8-bit sRGB.
fn size_and_middle(frame: &RecordedFrame) -> Option<(ISize, [u8; 4])> {
    let image = Image::from_encoded(Data::new_copy(&frame.png))?;
    let size = image.dimensions();
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        ColorSpace::new_srgb(),
    );
    let mut pixel = [0u8; 4];
    let middle = (size.width / 2, size.height / 2);
    let hint = skia::image::CachingHint::Disallow;
    image
        .read_pixels(&info, &mut pixel, 4, middle, hint)
        .then_some((size, pixel))
}

#[test]
fn a_test_platform_connects_only_inside_a_script() {
    assert!(matches!(TestPlatform::connect(), Err(Error::NoScript)));
}

#[test]
fn an_empty_script_closes_the_windows_and_ends_the_run() -> TestResult {
    let app = app(|_| {})?;
    let (ran, frames) = HostScript::new(Density::ONE).serve(|| run::<TestPlatform>(app));
    ran?;
    assert!(frames?.frames().is_empty());
    Ok(())
}

#[test]
fn a_scripted_key_reaches_the_window_that_has_the_keyboard_focus() -> TestResult {
    let keys = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&keys);
    let app = app(|_| {})?.on_keyboard(move |key| {
        seen.borrow_mut().push((key.code, key.pressed, key.repeat));
        false
    });
    let script = HostScript::new(Density::ONE)
        .focus_keyboard(KEYBOARD, APP_ID)
        .report(key_a());
    let (ran, _frames_without_a_recording) = script.serve(|| run::<TestPlatform>(app));
    ran?;
    assert_eq!(*keys.borrow(), [(KEY_A, true, false)]);
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
    let (ran, frames) = script.serve(|| run::<TestPlatform>(app));
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
    let (ran, frames) = script.serve(|| run::<TestPlatform>(app));
    ran?;
    assert!(frames?.frames().is_empty());
    Ok(())
}

#[test]
fn a_recording_contains_the_pixels_of_the_frame_shown_by_each_window() -> TestResult {
    let app = app(|ui| ui.spacer(100.dp(), 100.dp(), Modifier::new().background(BLUE)))?;
    let script = HostScript::new(Density::ONE).record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|| run::<TestPlatform>(app));
    ran?;
    // The renderer paints into a surface with more than 8 bits per channel, so a channel can round to the
    // neighbouring value when the frame is encoded.
    let near_blue = |pixel: [u8; 4]| {
        (pixel.into_iter().zip([0x30, 0x60, 0x90, 0xff]))
            .all(|(actual, blue)| actual.abs_diff(blue) <= 1)
    };
    let shown: Vec<_> = (frames?.frames().iter())
        .map(|frame| size_and_middle(frame).map(|(size, pixel)| (size, near_blue(pixel))))
        .collect();
    assert_eq!(shown, [Some((ISize::new(100, 100), true))]);
    Ok(())
}

/// An app whose blue bar animates its width for 100 ms with a linear tween.
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

fn presentation_times(script: HostScript, app: App) -> Result<Vec<Duration>, TestError> {
    let (ran, frames) = script.serve(|| run::<TestPlatform>(app));
    ran?;
    Ok((frames?.frames().iter())
        .map(|frame| frame.time.presentation)
        .collect())
}

#[test]
fn a_recording_repeats_at_each_interval_until_the_motion_rests() -> TestResult {
    let every = constant!(FrameInterval::per_second(40));
    let script = HostScript::new(Density::ONE).record_until_rest(every);
    assert_eq!(
        presentation_times(script, growing_bar()?)?,
        [0, 25, 50, 75, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn an_app_that_needs_a_missing_capability_fails_before_the_run_starts() -> TestResult {
    let mut app = App::new(lavashell_test_fixtures::facts());
    let window = Window::new("test", APP_ID).drag_region(|_, _| true);
    app.window(window.size(100.dp(), 100.dp()), |_| {})?;
    let (ran, frames) = HostScript::new(Density::ONE).serve(|| run::<TestPlatform>(app));
    assert!(matches!(
        ran,
        Err(RunError::Unsupported(missing)) if missing == BTreeSet::from([Capability::WindowMove])
    ));
    assert!(matches!(frames, Err(Error::NotServed(_))));
    Ok(())
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
        HostScript::new(Density::ONE).serve(|| run::<TestPlatform>(app));
    watched.replace(Ok(()))?;
    assert!(matches!(
        ran,
        Err(RunError::Platform(Error::Unsupported(request)))
            if matches!(*request, PlatformRequest::Watch(WatchRequest::Watch(..)))
    ));
    Ok(())
}
