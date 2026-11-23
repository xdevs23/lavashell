// SPDX-License-Identifier: GPL-3.0-only
use crate::{FrameInterval, RecordedFrame, RecordingPlatform};
use lavashell::renderer::skia::{
    self, AlphaType, ColorSpace, ColorType, Data, ISize, Image, ImageInfo,
};
use lavashell_core::prelude::{Color, Density, DpExt, Modifier, Ui, constant};
use lavashell_runtime::window::Window;
use lavashell_runtime::{App, RunError, SetupError, run};
use lavashell_test_platform::{Error, HostScript, PacedLinks};

const BLUE: Color = Color::rgb(0x0030_6090);
const COLOR_CHANNEL_ROUNDING: u8 = 1;
const SIXTY_HERTZ: FrameInterval = constant!(FrameInterval::per_second(60));

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Run(#[from] RunError<RecordingPlatform>),
    #[error(transparent)]
    Platform(#[from] Error),
}

type TestResult = Result<(), TestError>;

fn app(compose: impl FnMut(&mut Ui) + 'static) -> Result<App, SetupError> {
    let mut app = App::new().facts(lavashell_test_fixtures::facts());
    app.window(
        Window::new("test", "test.app").size(100.dp(), 100.dp()),
        compose,
    )?;
    Ok(app)
}

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
fn a_recording_contains_the_pixels_of_the_frame_shown_by_each_window() -> TestResult {
    let app = app(|ui| ui.spacer(100.dp(), 100.dp(), Modifier::new().background(BLUE)))?;
    let script = HostScript::new(Density::ONE).record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<RecordingPlatform>(app, host));
    ran?;
    let near_blue = |pixel: [u8; 4]| {
        (pixel.into_iter().zip([0x30, 0x60, 0x90, 0xff]))
            .all(|(actual, blue)| actual.abs_diff(blue) <= COLOR_CHANNEL_ROUNDING)
    };
    let shown: Vec<_> = (frames?.frames().iter())
        .map(|frame| size_and_middle(frame).map(|(size, pixel)| (size, near_blue(pixel))))
        .collect();
    assert_eq!(shown, [Some((ISize::new(100, 100), true))]);
    Ok(())
}

#[test]
fn a_default_pace_records_one_frame_at_the_end_decoded_at_the_painted_size() -> TestResult {
    let app = app(|_| {})?;
    let script = HostScript::new(Density::ONE).record_paced(PacedLinks::default());
    let (ran, frames) = script.serve(|host| run::<RecordingPlatform>(app, host));
    ran?;
    let sizes: Vec<_> = (frames?.frames().iter())
        .map(|frame| size_and_middle(frame).map(|(size, _)| size))
        .collect();
    assert_eq!(sizes, [Some(ISize::new(100, 100))]);
    Ok(())
}
