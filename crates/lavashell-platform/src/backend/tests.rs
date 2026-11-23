// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_renderer::text::{Hinting, TextAntialiasing, TextDestination, TextRendering};

#[test]
fn a_painter_without_a_device_shapes_text_as_its_setup_says()
-> Result<(), lavashell_renderer::Error> {
    let rendering = TextRendering {
        hinting: Hinting::None,
        fractional_positioning: true,
        antialiasing: TextAntialiasing::Aliased,
    };
    let setup = RendererSetup {
        fonts: Fonts::new(),
        painter: Some(Box::new(move |painter: &mut Painter| {
            painter.set_text_rendering(rendering, TextDestination::default());
        })),
        frame_timing: FrameTiming::Off,
        damage: DamageMode::Tracked,
    };
    assert_eq!(setup.painter_without_device()?.text_rendering(), rendering);
    Ok(())
}
