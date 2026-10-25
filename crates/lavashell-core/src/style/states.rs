// SPDX-License-Identifier: GPL-3.0-only
use super::BorderStyle;
use crate::animation::{Composite, Fading, MotionSpec, composite_fields};
use crate::color::{ColorIn, ColorInterpolation};
use crate::effects::{Interpolated, Material, ShapedMaterial, Stroke};
use crate::geometry::Color;
use crate::shape::Corners;
use crate::units::{Dp, Fraction, ScaleFactor};
use std::{fmt, rc::Rc};

/// The interaction state of a node as separate flags. The application defines which flag wins over
/// another and how each combination looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub pointer: PointerState,
    pub focus: FocusState,
    pub disabled: bool,
    /// The selected state that the application sets with `Modifier::selected`.
    /// Input, focus and the disabled state do not change it.
    pub selected: bool,
}

/// What the pointer does to a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerState {
    pub hovered: bool,
    pub active: bool,
    /// Whether the node has captured the pointer. The capture continues when a drag leaves the hit
    /// region of the node.
    pub captured: bool,
}

/// Whether a node has the focus, and shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusState {
    pub focused: bool,
    /// Whether the node is focused and shows its focus. A text field or a text area shows the focus
    /// after any input that focused it. Every other node shows the focus when a key or the
    /// accessibility action for focus focused it, or when it got the focus before any input. A
    /// pointer press hides the focus again.
    pub visible: bool,
}

/// How a node looks in one interaction state: a material below the decorations of the node, a tint
/// under its content, an outline above its content, the color of the text and icons inside it, and a
/// scale and an opacity for everything the node paints. None of these changes layout or input.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractionAppearance {
    tint: Option<(Color, Corners)>,
    outline: BorderStyle,
    material: Option<ShapedMaterial>,
    content: Option<Color>,
    scale: ScaleFactor,
    opacity: Fraction,
}

impl Default for InteractionAppearance {
    fn default() -> Self {
        Self {
            tint: None,
            outline: BorderStyle::NONE,
            material: None,
            content: None,
            scale: ScaleFactor::ONE,
            opacity: crate::constant!(Fraction::try_new(1.0)),
        }
    }
}

impl InteractionAppearance {
    #[must_use]
    pub fn tint(mut self, color: Color, corners: impl Into<Corners>) -> Self {
        self.tint = Some((color, corners.into()));
        self
    }

    /// An outline painted by this appearance takes the place of the focus ring set in
    /// `InteractionStyle::focus`.
    #[must_use]
    pub fn outline(mut self, outline: BorderStyle) -> Self {
        self.outline = outline;
        self
    }

    #[must_use]
    pub fn material(mut self, material: Material, corners: impl Into<Corners>) -> Self {
        self.material = Some(ShapedMaterial::new(material, corners));
        self
    }

    /// The color of text and icons inside the node and its descendants, down to the next descendant
    /// whose appearance sets a content color.
    #[must_use]
    pub fn content(mut self, color: Color) -> Self {
        self.content = Some(color);
        self
    }

    /// The scale about the center of the node.
    #[must_use]
    pub fn scale(mut self, scale: ScaleFactor) -> Self {
        self.scale = scale;
        self
    }

    #[must_use]
    pub fn opacity(mut self, opacity: Fraction) -> Self {
        self.opacity = opacity;
        self
    }
}

/// The content color of a state, applied with `weight`. At 0, text and icons keep the color they
/// have without this state. At 1, they are drawn in `color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContentColor {
    color: ColorIn,
    weight: f64,
}

impl ContentColor {
    pub fn applied_to(&self, color: Color) -> Color {
        let ColorIn {
            color: content,
            interpolation,
        } = self.color;
        color.mix_in(content, self.weight, interpolation)
    }
}

/// The content colors of the state appearances around a node, outermost first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ContentColors(Vec<ContentColor>);

impl ContentColors {
    pub const NONE: Self = Self(Vec::new());

    pub fn pushed(&self, content: ContentColor) -> Self {
        let mut colors = self.0.clone();
        colors.push(content);
        Self(colors)
    }

    pub fn applied_to(&self, color: Color) -> Color {
        (self.0.iter()).fold(color, |color, content| content.applied_to(color))
    }
}

impl Composite for ContentColor {
    type Parts = (ColorIn, f64);

    fn parts(&self) -> (ColorIn, f64) {
        (self.color, self.weight)
    }

    fn with_parts(&self, (color, weight): (ColorIn, f64)) -> Self {
        Self { color, weight }
    }
}

impl Fading for ContentColor {
    fn faded(&self) -> Self {
        Self {
            weight: 0.0,
            ..*self
        }
    }
}

composite_fields! {
    pub(crate) struct Tint {
        pub color: ColorIn,
        pub corners: Corners,
    }
}

/// When a tint appears or disappears, the transparent tint at the missing end has the corners of the
/// present tint.
impl Fading for Tint {
    fn faded(&self) -> Self {
        Self {
            color: ColorIn {
                color: self.color.color.with_alpha(0.0),
                ..self.color
            },
            corners: self.corners,
        }
    }
}

composite_fields! {
    pub(crate) struct Outline {
        pub stroke: Interpolated<Stroke>,
        pub radius: Dp,
    }
}

/// When an outline appears or disappears, the faded outline at the missing end has the radius of the
/// present outline.
impl Fading for Outline {
    fn faded(&self) -> Self {
        Self {
            stroke: self.stroke.faded(),
            radius: self.radius,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct AnimatedAppearance {
    pub tint: Option<Tint>,
    pub outline: Option<Outline>,
    pub material: Option<Interpolated<ShapedMaterial>>,
    pub content: Option<ContentColor>,
    pub scale: ScaleFactor,
    pub opacity: Fraction,
}

composite_fields! {
    pub(crate) struct AnimatedAppearanceParts {
        tint: Option<Tint>,
        outline: Option<Outline>,
        material: Option<Interpolated<ShapedMaterial>>,
        content: Option<ContentColor>,
        scale: ScaleFactor,
        opacity: f64,
    }
}

/// Corner radii below zero and opacities outside `0..=1` become the nearest valid value. An opacity
/// that is not a number keeps the opacity of `self`.
impl Composite for AnimatedAppearance {
    type Parts = AnimatedAppearanceParts;

    fn parts(&self) -> AnimatedAppearanceParts {
        AnimatedAppearanceParts {
            tint: self.tint.clone(),
            outline: self.outline.clone(),
            material: self.material.clone(),
            content: self.content,
            scale: self.scale,
            opacity: self.opacity.get(),
        }
    }

    fn with_parts(&self, parts: AnimatedAppearanceParts) -> Self {
        let tint = parts.tint.map(|tint| Tint {
            corners: tint.corners.map(|radius| radius.max(Dp::ZERO)),
            ..tint
        });
        let outline = parts.outline.map(|outline| Outline {
            radius: outline.radius.max(Dp::ZERO),
            ..outline
        });
        Self {
            tint,
            outline,
            material: parts.material,
            content: parts.content,
            scale: parts.scale,
            opacity: Fraction::saturating(parts.opacity).unwrap_or(self.opacity),
        }
    }
}

impl AnimatedAppearance {
    fn new(appearance: InteractionAppearance, interpolation: ColorInterpolation) -> Self {
        let InteractionAppearance {
            tint,
            outline,
            material,
            content,
            scale,
            opacity,
        } = appearance;
        let stroke = outline.stroke();
        Self {
            tint: tint.map(|(color, corners)| Tint {
                color: color.interpolated(interpolation),
                corners,
            }),
            outline: stroke.paints().then(|| Outline {
                stroke: Interpolated::new(stroke, interpolation),
                radius: outline.radius,
            }),
            material: material.map(|material| Interpolated::new(material, interpolation)),
            content: content.map(|color| ContentColor {
                color: color.interpolated(interpolation),
                weight: 1.0,
            }),
            scale,
            opacity,
        }
    }
}

/// Styles for interaction states, defined by the application as a function from state to appearance.
/// Changes between appearances animate on the motion clock of the view and follow its reduced-motion
/// setting.
#[derive(Clone)]
pub struct StateStyles {
    resolve: Rc<dyn Fn(InteractionState) -> InteractionAppearance>,
    pub(crate) motion: MotionSpec,
    color_interpolation: ColorInterpolation,
}

impl StateStyles {
    /// The view calls `resolve` only when the interaction state of a node or its state styles change.
    /// The appearance returned by `resolve` must therefore depend on the state alone. Build the styles
    /// again when other data that they depend on changes.
    pub fn new(resolve: impl Fn(InteractionState) -> InteractionAppearance + 'static) -> Self {
        Self {
            resolve: Rc::new(resolve),
            motion: MotionSpec::Instant,
            color_interpolation: ColorInterpolation::default(),
        }
    }

    /// Every part of the appearance moves to the appearance of a new state with `motion`.
    #[must_use]
    pub fn motion(mut self, motion: impl Into<MotionSpec>) -> Self {
        self.motion = motion.into();
        self
    }

    #[must_use]
    pub fn color_interpolation(mut self, interpolation: ColorInterpolation) -> Self {
        self.color_interpolation = interpolation;
        self
    }

    pub(crate) fn appearance(&self, state: InteractionState) -> AnimatedAppearance {
        AnimatedAppearance::new((self.resolve)(state), self.color_interpolation)
    }
}

impl fmt::Debug for StateStyles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateStyles")
            .field("motion", &self.motion)
            .field("color_interpolation", &self.color_interpolation)
            .finish_non_exhaustive()
    }
}

impl PartialEq for StateStyles {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.resolve, &other.resolve)
            && self.motion == other.motion
            && self.color_interpolation == other.color_interpolation
    }
}
