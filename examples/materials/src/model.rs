// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::{
    materials::{SurfaceTreatment, TreatmentParameters},
    prelude::*,
};
use lavashell_example_support::{Theme, ThemeChoice, icon, lucide};
use std::time::Duration;

pub const PANEL: Size = Size::new(Dp::new(355.0), Dp::new(225.0));
const START: Point = Point::new(Dp::new(215.0), Dp::new(118.0));
const RADIUS: Dp = Dp::new(28.0);
const TREATMENT: SurfaceTreatment = SurfaceTreatment::RefractiveGlass;
const HUE: f64 = 80.0;
const CHROMA: f64 = 0.035;

#[derive(Clone)]
pub struct Demo {
    pub treatment: State<SurfaceTreatment>,
    pub blur: State<Dp>,
    pub opacity: State<f64>,
    pub radius: State<Dp>,
    pub refraction: State<Dp>,
    pub hue: State<f64>,
    pub chroma: State<f64>,
    pub position: State<Point>,
    pub dragging: State<Option<Point>>,
    pub animate: State<bool>,
    pub phase: Animation<f64>,
    pub request: State<OutputRequest>,
    pub output: State<Option<OutputInfo>>,
    pub blend: State<BlendMode>,
    pub play: &'static Icon,
    pub pause: &'static Icon,
    pub theme: Theme,
    phase_frames: Keyframes<f64>,
}

impl Demo {
    pub fn new(autoplay: bool, theme: ThemeChoice) -> Result<Self, Error> {
        let phase_frames = Keyframes::new(vec![
            Keyframe::new(Duration::ZERO, 0.0).easing(Easing::Linear),
            Keyframe::new(Duration::from_secs(8), 1.0),
        ])
        .map_err(Error::PhaseLoop)?;
        let preset = TREATMENT.parameters();
        Ok(Self {
            treatment: State::new(TREATMENT),
            blur: State::new(preset.blur.map_or(Dp::ZERO, NonNegativeDp::get)),
            opacity: State::new(preset.opacity),
            radius: State::new(RADIUS),
            refraction: State::new(preset.refraction.map_or(Dp::ZERO, NonNegativeDp::get)),
            hue: State::new(HUE),
            chroma: State::new(CHROMA),
            position: State::new(START),
            dragging: State::new(None),
            animate: State::new(autoplay),
            phase: Animation::new(0.0).map_err(Error::PhaseMotion)?,
            request: State::new(OutputRequest::default()),
            output: State::new(None),
            blend: State::new(BlendMode::Multiply),
            play: icon(&lucide::PLAY)?,
            pause: icon(&lucide::PAUSE)?,
            theme: Theme::new(theme)?,
            phase_frames,
        })
    }

    pub fn select(&self, treatment: SurfaceTreatment) {
        self.treatment.set(treatment);
        let preset = treatment.parameters();
        self.blur
            .set(preset.blur.map_or(Dp::ZERO, NonNegativeDp::get));
        self.opacity.set(preset.opacity);
    }

    pub fn reset(&self) {
        self.select(self.treatment.get());
        self.radius.set(RADIUS);
        if let Some(refraction) = self.treatment.get().parameters().refraction {
            self.refraction.set(refraction.get());
        }
        self.position.set(START);
    }

    pub fn material(&self) -> Result<Material, NumberError> {
        let tint = tint(self.hue.get(), self.chroma.get());
        let parameters = TreatmentParameters {
            blur: Some(NonNegativeDp::saturating(self.blur.get())?),
            opacity: self.opacity.get(),
            refraction: Some(NonNegativeDp::saturating(self.refraction.get())?),
        };
        Ok(self.treatment.get().material_with(tint, PANEL, parameters))
    }

    #[must_use]
    pub fn panel_position(&self, size: Size) -> Point {
        inside(self.position.get(), size)
    }

    pub fn motion(&self, ui: &mut Ui) {
        if let Err(error) = ui.motion().bind(&self.phase) {
            ui.fail(error);
        }
        if self.animate.get() && self.phase.is_finished() {
            self.phase.play(
                self.phase_frames.clone(),
                Playback {
                    repeat: Repeat::Forever,
                    direction: Direction::Forward,
                },
            );
        } else if !self.animate.get() {
            self.phase.pause();
        }
        if self.animate.get() {
            self.phase.resume();
        }
    }
}

fn tint(hue: f64, chroma: f64) -> Color {
    Color::oklch(0.93, chroma, hue).mapped(Gamut::Srgb)
}

#[must_use]
pub fn inside(position: Point, size: Size) -> Point {
    Point::new(
        position
            .x
            .clamp(Dp::ZERO, (size.width - PANEL.width).max(Dp::ZERO)),
        position
            .y
            .clamp(Dp::ZERO, (size.height - PANEL.height).max(Dp::ZERO)),
    )
}
