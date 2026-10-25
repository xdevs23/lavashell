// SPDX-License-Identifier: GPL-3.0-only
//! The effects with which a node is painted, such as strokes, shadows, backdrop blur, glass and blend modes. Every
//! renderer gets them in the same form.

use crate::{
    geometry::{Color, FinitePoint, Point},
    output::finest_step,
    scene::{Affine, Brush, DrawCommand},
    shape::{Corners, Shape},
    units::{Dp, FiniteDp, NonNegative, NonNegativeDp, NumberError},
};

mod motion;
#[cfg(test)]
mod tests;

pub(crate) use motion::Interpolated;

/// How many standard deviations away from its source a Gaussian blur still changes a pixel of a target whose
/// channel values lie `channel_step` apart. Beyond `x` standard deviations the tail of a Gaussian is below
/// `exp(-x^2 / 2)`. The function returns the `x` at which that bound falls to half a step, a change that the target
/// does not show. A step finer than one step at the highest bit depth of any output is replaced by that step,
/// because no output shows a finer change.
#[must_use]
pub fn blur_reach(channel_step: f64) -> f64 {
    (-2.0 * (channel_step.max(finest_step()) / 2.0).ln()).sqrt()
}

/// A filter that the renderer applies to a drawn subtree. `id` selects the filter registered by the renderer under
/// that number, and `parameters` configure it. `outset` must be at least the distance by which the filter draws
/// outside the unfiltered subtree.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFilter {
    pub id: u64,
    pub parameters: Vec<f64>,
    pub outset: NonNegativeDp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    Clear,
    Source,
    Destination,
    #[default]
    SourceOver,
    DestinationOver,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    Xor,
    Plus,
    Modulate,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Multiply,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderPlacement {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: NonNegativeDp,
    pub placement: BorderPlacement,
}

impl Stroke {
    /// A stroke placed inside the outline. A negative width is set to zero, so the stroke draws nothing.
    ///
    /// # Errors
    ///
    /// Fails when the width is not finite.
    pub const fn new(color: Color, width: Dp) -> Result<Self, NumberError> {
        match NonNegativeDp::saturating(width) {
            Ok(width) => Ok(Self {
                color,
                width,
                placement: BorderPlacement::Inside,
            }),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn placement(mut self, placement: BorderPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub(crate) fn paints(&self) -> bool {
        self.width.get() > Dp::ZERO && self.color.alpha() > 0.0
    }

    /// The signed distances from the outline that the stroke covers. A negative distance lies inside the shape.
    #[must_use]
    pub fn distances(&self) -> std::ops::Range<Dp> {
        let width = self.width.get();
        match self.placement {
            BorderPlacement::Inside => -width..Dp::ZERO,
            BorderPlacement::Center => -width / 2.0..width / 2.0,
            BorderPlacement::Outside => Dp::ZERO..width,
        }
    }

    #[must_use]
    pub fn reach(&self) -> Dp {
        self.distances().end
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub offset: FinitePoint,
    pub blur: NonNegativeDp,
    pub spread: FiniteDp,
    pub inset: bool,
}

impl Shadow {
    /// `blur` is the sigma of the Gaussian blur of the shadow. A negative blur is set to zero, which draws a sharp
    /// shadow.
    ///
    /// # Errors
    ///
    /// Fails when the blur or a coordinate of the offset is not finite.
    pub const fn new(color: Color, blur: Dp, offset: Point) -> Result<Self, NumberError> {
        let blur = match NonNegativeDp::saturating(blur) {
            Ok(blur) => blur,
            Err(error) => return Err(error),
        };
        match FinitePoint::try_new(offset) {
            Ok(offset) => Ok(Self {
                color,
                blur,
                offset,
                spread: FiniteDp::ZERO,
                inset: false,
            }),
            Err(error) => Err(error),
        }
    }

    /// # Errors
    ///
    /// Fails when the spread is not finite.
    pub const fn spread(mut self, spread: Dp) -> Result<Self, NumberError> {
        match FiniteDp::try_new(spread) {
            Ok(spread) => {
                self.spread = spread;
                Ok(self)
            }
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    /// How far the shadow extends beyond its shape through its blur, spread and offset, on a target whose channel
    /// values lie `channel_step` apart.
    #[must_use]
    pub fn reach(&self, channel_step: f64) -> Dp {
        let offset = self.offset.get();
        self.blur.get() * blur_reach(channel_step)
            + self.spread.get().abs()
            + offset.x.abs().max(offset.y.abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// The largest distance by which the lens shifts the backdrop seen through it.
    pub refraction: NonNegativeDp,
    pub dispersion: NonNegativeDp,
    pub rim: NonNegative,
}

impl Glass {
    #[must_use]
    pub fn reach(&self) -> Dp {
        self.refraction.get() + self.dispersion.get()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Backdrop {
    pub blur: NonNegativeDp,
    pub glass: Option<Glass>,
}

impl Backdrop {
    /// How far beyond the shape the blur and the lens sample the backdrop, on a target whose channel values lie
    /// `channel_step` apart.
    #[must_use]
    pub fn reach(&self, channel_step: f64) -> Dp {
        self.blur.get() * blur_reach(channel_step)
            + self.glass.map_or(Dp::ZERO, |glass| glass.reach())
    }

    /// A backdrop blurred with the Gaussian `sigma`. A negative sigma is set to zero, which draws the backdrop
    /// without blur.
    ///
    /// # Errors
    ///
    /// Fails when the sigma is not finite.
    pub const fn blur(sigma: Dp) -> Result<Self, NumberError> {
        match NonNegativeDp::saturating(sigma) {
            Ok(blur) => Ok(Self { blur, glass: None }),
            Err(error) => Err(error),
        }
    }

    /// A backdrop blurred with the Gaussian `sigma` and seen through `glass`. A negative sigma is set to zero, as
    /// in `Backdrop::blur`.
    ///
    /// # Errors
    ///
    /// Fails when the sigma is not finite.
    pub const fn glass(sigma: Dp, glass: Glass) -> Result<Self, NumberError> {
        match NonNegativeDp::saturating(sigma) {
            Ok(blur) => Ok(Self {
                blur,
                glass: Some(glass),
            }),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub fill: Brush,
    pub backdrop: Option<Backdrop>,
    pub shading: Option<Brush>,
    pub border: Option<Stroke>,
    pub shadows: Vec<Shadow>,
}

impl Material {
    pub fn new(fill: impl Into<Brush>) -> Self {
        Self {
            fill: fill.into(),
            backdrop: None,
            shading: None,
            border: None,
            shadows: Vec::new(),
        }
    }

    #[must_use]
    pub fn fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    #[must_use]
    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    #[must_use]
    pub fn shading(mut self, shading: impl Into<Brush>) -> Self {
        self.shading = Some(shading.into());
        self
    }

    #[must_use]
    pub fn border(mut self, border: Stroke) -> Self {
        self.border = Some(border);
        self
    }

    #[must_use]
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    /// The commands that paint the material on `shape`. Brushes are placed
    /// relative to `origin`.
    pub(crate) fn commands(&self, shape: &Shape, origin: Point) -> MaterialCommands {
        let shadows = |inset: bool| {
            self.shadows
                .iter()
                .filter(move |s| s.inset == inset)
                .map(|shadow| DrawCommand::Shadow {
                    shape: shape.clone(),
                    shadow: *shadow,
                })
        };
        let backdrop = self.backdrop.map(|effect| DrawCommand::Backdrop {
            shape: shape.clone(),
            effect,
        });
        let fills = std::iter::once(&self.fill)
            .chain(&self.shading)
            .map(|brush| DrawCommand::Fill {
                shape: shape.clone(),
                brush: brush.clone().translated(origin),
            });
        let border = self.border.map(|stroke| DrawCommand::Stroke {
            shape: shape.clone(),
            stroke,
        });
        MaterialCommands {
            below: shadows(false).chain(backdrop).chain(fills).collect(),
            enclosing: shadows(true).chain(border).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapedMaterial {
    pub material: Material,
    pub corners: Corners,
}

impl ShapedMaterial {
    pub fn new(material: Material, corners: impl Into<Corners>) -> Self {
        Self {
            material,
            corners: corners.into(),
        }
    }
}

/// The draw commands of a material in two lists: the commands painted below the content of the node, and the
/// inset shadows and the border painted over the content.
pub(crate) struct MaterialCommands {
    below: Vec<DrawCommand>,
    enclosing: Vec<DrawCommand>,
}

impl MaterialCommands {
    pub fn transformed(self, transform: Affine) -> Self {
        if transform == Affine::IDENTITY {
            return self;
        }
        let wrap = |inner: Vec<DrawCommand>| {
            if inner.is_empty() {
                return inner;
            }
            let open = [DrawCommand::Save, DrawCommand::Transform(transform)];
            open.into_iter()
                .chain(inner)
                .chain([DrawCommand::Restore])
                .collect()
        };
        Self {
            below: wrap(self.below),
            enclosing: wrap(self.enclosing),
        }
    }

    /// Appends the commands below the content to `commands`. Pushes the commands over the content onto `unwind` in
    /// reverse order, because the caller paints `unwind` in reverse after the content.
    pub fn paint(self, commands: &mut Vec<DrawCommand>, unwind: &mut impl Extend<DrawCommand>) {
        commands.extend(self.below);
        unwind.extend(self.enclosing.into_iter().rev());
    }
}
