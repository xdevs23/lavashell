// SPDX-License-Identifier: GPL-3.0-only
use super::settings::NodeSetting;
use super::{Bounds, Deferred, Node, Read, Ui, Used};
use crate::animation::{Animatable, Animation, BindError, Bindable, Motion};
use crate::effects::{
    Backdrop, BorderPlacement, Material, Shadow, ShapedMaterial, SourceFilter, Stroke,
};
use crate::geometry::Rect;
use crate::geometry::{Axis, Color, Insets, Point, Size};
use crate::scene::Brush;
use crate::scroll::ScrollState;
use crate::shape::Corners;
use crate::units::{Dp, FiniteDp, Fraction, NonNegativeDp, NumberError};
use std::fmt;
use std::rc::Rc;

/// An animation whose value paints as a color.
pub trait AnimatedColor: Bindable {
    fn color(&self) -> Color;
}

impl<T: Animatable + Into<Color>> AnimatedColor for Animation<T> {
    fn color(&self) -> Color {
        self.get().into()
    }
}

#[derive(Clone)]
pub enum ModifierElement {
    Padding(Insets),
    Width(Deferred<Dp>),
    Height(Deferred<Dp>),
    MinSize(Size),
    MaxSize(Size),
    MeasurementLimit(Size),
    FillWidth,
    FillHeight,
    Background(Brush),
    Border {
        stroke: Stroke,
        corners: Corners,
    },
    Clip(Corners),
    ClipRect(Rect, Corners),
    Effect {
        blur: NonNegativeDp,
        filter: Option<SourceFilter>,
    },
    Backdrop {
        effect: Backdrop,
        corners: Corners,
    },
    Shadow {
        shadow: Shadow,
        corners: Corners,
    },
    Outline {
        stroke: Stroke,
        corners: Corners,
        offset: Dp,
    },
    Material(ShapedMaterial),
    MaterialGroup {
        material: Material,
        spacing: NonNegativeDp,
    },
    Island(Corners),
    Opacity(Fraction),
    Offset(Deferred<Point, Bounds>),
    Position(Deferred<Point, Bounds>),
    ShownWhen(Deferred<bool>),
    AnimatedOffset(Animation<Point>),
    AnimatedOpacity(Animation<f64>),
    AnimatedBackground(Rc<dyn AnimatedColor>),
    FollowScroll {
        scroll: ScrollState,
        travel: Dp,
    },
    /// A value given to the modifier that is not valid, which fails the layout of the view.
    Invalid {
        value: ModifierValue,
        cause: NumberError,
    },
}

/// The kind of a value given to a modifier. The error for a value that is not valid contains this
/// kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierValue {
    MeasurementLimit,
    MinimumSize,
    BorderWidth,
    Blur,
    OutlineWidth,
    MaterialGroupSpacing,
    Opacity,
}

impl fmt::Display for ModifierValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MeasurementLimit => "measurement limit",
            Self::MinimumSize => "minimum size",
            Self::BorderWidth => "border width",
            Self::Blur => "blur",
            Self::OutlineWidth => "outline width",
            Self::MaterialGroupSpacing => "material group spacing",
            Self::Opacity => "opacity",
        })
    }
}

/// How a node looks, is laid out and behaves: the elements that wrap its content, in order, and
/// the settings of the node, applied in order when the node is added.
#[derive(Clone, Default)]
#[must_use = "a modifier changes a node only when it is given to the call that adds the node"]
pub struct Modifier {
    elements: Vec<ModifierElement>,
    settings: Vec<NodeSetting>,
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(mut self, element: ModifierElement) -> Self {
        self.elements.push(element);
        self
    }

    pub(super) fn set(mut self, setting: NodeSetting) -> Self {
        self.settings.push(setting);
        self
    }

    /// Adds the element made by `element` from the value in `checked`. When `checked` is an error,
    /// adds an `Invalid` element for `value` instead.
    fn checked<T>(
        self,
        value: ModifierValue,
        checked: Result<T, NumberError>,
        element: impl FnOnce(T) -> ModifierElement,
    ) -> Self {
        self.push(match checked {
            Ok(valid) => element(valid),
            Err(cause) => ModifierElement::Invalid { value, cause },
        })
    }

    pub fn padding(self, value: Dp) -> Self {
        self.insets(Insets::all(value))
    }

    pub fn padding_xy(self, horizontal: Dp, vertical: Dp) -> Self {
        self.insets(Insets::xy(horizontal, vertical))
    }

    pub fn insets(self, value: Insets) -> Self {
        self.push(ModifierElement::Padding(value))
    }

    /// Measures the content of the node against `limit`, apart from the constraints of the parent.
    /// The node itself still gets a size within the constraints of the parent. An axis left
    /// unbounded by the parent is still unbounded for the node. Content larger than the node is
    /// clipped only by a clip set on the node. A limit that is not finite fails the layout of the
    /// view.
    pub fn measurement_limit(self, limit: Size) -> Self {
        let limit = finite(limit)
            .map(|limit| Size::new(limit.width.max(Dp::ZERO), limit.height.max(Dp::ZERO)));
        self.checked(
            ModifierValue::MeasurementLimit,
            limit,
            ModifierElement::MeasurementLimit,
        )
    }

    /// A negative width is zero.
    pub fn width(self, value: impl Into<Deferred<Dp>>) -> Self {
        self.push(ModifierElement::Width(value.into()))
    }

    /// A negative height is zero.
    pub fn height(self, value: impl Into<Deferred<Dp>>) -> Self {
        self.push(ModifierElement::Height(value.into()))
    }

    pub fn size(self, width: impl Into<Deferred<Dp>>, height: impl Into<Deferred<Dp>>) -> Self {
        self.width(width).height(height)
    }

    /// A minimum size that is not finite fails the layout of the view.
    pub fn min_size(self, width: Dp, height: Dp) -> Self {
        let size = finite(Size::new(width, height));
        self.checked(ModifierValue::MinimumSize, size, ModifierElement::MinSize)
    }

    /// Limits the width to `width`. Narrower content is not stretched to it.
    pub fn max_width(self, width: Dp) -> Self {
        self.max_size(Size::new(width, Dp::INFINITY))
    }

    /// Limits the height to `height`. Shorter content is not stretched to it.
    pub fn max_height(self, height: Dp) -> Self {
        self.max_size(Size::new(Dp::INFINITY, height))
    }

    /// Sets a minimum size `min` and a maximum size `max`, inside the constraints set by the
    /// parent. A minimum larger than the maximum is lowered to the maximum.
    pub fn size_in(self, min: Size, max: Size) -> Self {
        self.max_size(max).min_size(min.width, min.height)
    }

    fn max_size(self, max: Size) -> Self {
        self.push(ModifierElement::MaxSize(Size::new(
            max.width.max(Dp::ZERO),
            max.height.max(Dp::ZERO),
        )))
    }

    /// Makes the width as large as the parent proposes. In a row, a child without a weight shares
    /// the width left by the other children, like `weight_up_to(1.0)`. A width bound set before
    /// the fill still limits the child. A width set before the fill takes precedence over it. In a
    /// row that scrolls, the child has its minimum width. An overlay without a weight and a width,
    /// such as the one made by [`Ui::key`], gets the same weight when a child in it fills.
    pub fn fill_max_width(self) -> Self {
        self.push(ModifierElement::FillWidth)
    }

    /// Makes the height as large as the parent proposes, by the rules of `fill_max_width` along
    /// the vertical axis. In a column that scrolls, the child has the minimum height.
    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillHeight)
    }

    pub fn background(self, color: impl Into<Brush>) -> Self {
        self.push(ModifierElement::Background(color.into()))
    }

    /// A negative width draws nothing, and one that is not finite fails the layout of the view.
    pub fn border(self, color: Color, width: Dp, corners: impl Into<Corners>) -> Self {
        let stroke = Stroke::new(color, width);
        self.checked(ModifierValue::BorderWidth, stroke, |stroke| {
            ModifierElement::Border {
                stroke,
                corners: corners.into(),
            }
        })
    }

    pub fn clip(self, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Clip(corners.into()))
    }

    /// Clips painting and input to `rect`, in the coordinates of the node, without changing the
    /// measured size of the node.
    pub fn clip_rect(self, rect: Rect, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::ClipRect(rect, corners.into()))
    }

    /// A negative sigma draws without blur, and one that is not finite fails the layout of the
    /// view.
    pub fn blur(self, sigma: Dp) -> Self {
        let blur = NonNegativeDp::saturating(sigma);
        self.checked(ModifierValue::Blur, blur, |blur| ModifierElement::Effect {
            blur,
            filter: None,
        })
    }

    /// Paints the subtree of the node into a separate layer and applies `filter` to that layer,
    /// without changing the layout.
    pub fn source_filter(self, filter: SourceFilter) -> Self {
        self.push(ModifierElement::Effect {
            blur: NonNegativeDp::ZERO,
            filter: Some(filter),
        })
    }

    pub fn backdrop(self, effect: Backdrop, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Backdrop {
            effect,
            corners: corners.into(),
        })
    }

    pub fn shadow(self, shadow: Shadow, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Shadow {
            shadow,
            corners: corners.into(),
        })
    }

    /// Paints an outline after the subtree, outside the bounds of the node. The outline does not
    /// change the measured size.
    /// A negative width draws nothing, and one that is not finite fails the layout of the view.
    pub fn outline(self, color: Color, width: Dp, corners: impl Into<Corners>, offset: Dp) -> Self {
        let stroke = Stroke::new(color, width);
        self.checked(ModifierValue::OutlineWidth, stroke, |stroke| {
            ModifierElement::Outline {
                stroke: stroke.placement(BorderPlacement::Outside),
                corners: corners.into(),
                offset,
            }
        })
    }

    pub fn material(self, material: Material, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Material(ShapedMaterial::new(
            material, corners,
        )))
    }

    /// Paints one material on the union of the direct children marked with `island`, at their
    /// animated bounds. Islands closer to each other than `spacing` merge into one surface. A
    /// negative spacing counts as zero, and one that is not finite fails the layout of the view.
    pub fn material_group(self, material: Material, spacing: Dp) -> Self {
        let spacing = NonNegativeDp::saturating(spacing);
        self.checked(ModifierValue::MaterialGroupSpacing, spacing, |spacing| {
            ModifierElement::MaterialGroup { material, spacing }
        })
    }

    /// Adds the bounds of this node, with `corners`, to the material group set on the parent.
    pub fn island(self, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Island(corners.into()))
    }

    /// Sets the opacity, clamped to 0 to 1. An opacity that is NaN fails the layout of the view.
    pub fn opacity(self, value: f64) -> Self {
        let opacity = Fraction::saturating(value);
        self.checked(ModifierValue::Opacity, opacity, ModifierElement::Opacity)
    }

    pub fn offset(
        self,
        x: impl Into<Deferred<Dp, Bounds>>,
        y: impl Into<Deferred<Dp, Bounds>>,
    ) -> Self {
        let by = x.into().zip(&y.into()).map(|(x, y)| Point::new(x, y));
        self.push(ModifierElement::Offset(by))
    }

    /// Moves the node to `at`, in the coordinates of the view, by offsetting it from its laid-out
    /// position.
    pub fn position(self, at: impl Into<Deferred<Point, Bounds>>) -> Self {
        self.push(ModifierElement::Position(at.into()))
    }

    /// Lays out the node while `condition` is true. Otherwise the node takes no room, is not
    /// painted, gets no input and is not in the accessibility tree.
    pub fn shown_when(self, condition: impl Into<Deferred<bool>>) -> Self {
        self.push(ModifierElement::ShownWhen(condition.into()))
    }

    pub fn animated_offset(self, value: Animation<Point>) -> Self {
        self.push(ModifierElement::AnimatedOffset(value))
    }

    pub fn animated_opacity(self, value: Animation<f64>) -> Self {
        self.push(ModifierElement::AnimatedOpacity(value))
    }

    /// Moves the node along the axis of `scroll` by the scrolled fraction of `travel`.
    pub fn follow_scroll(self, scroll: ScrollState, travel: Dp) -> Self {
        self.push(ModifierElement::FollowScroll { scroll, travel })
    }

    pub fn animated_background(self, value: impl AnimatedColor + 'static) -> Self {
        self.push(ModifierElement::AnimatedBackground(Rc::new(value)))
    }

    /// The elements and the settings of `other` after those of this modifier.
    pub fn then(mut self, other: Self) -> Self {
        self.elements.extend(other.elements);
        self.settings.extend(other.settings);
        self
    }

    /// Sets the elements of this modifier on `node`, applies its settings with the locals read from
    /// `ui`, and binds the animations read by the elements to the motion clock of `ui`.
    ///
    /// # Errors
    ///
    /// Fails when an element reads an animation bound to the clock of another view. The elements
    /// and the settings are applied to the node even then.
    pub(super) fn apply(self, node: &mut Node, ui: &Ui) -> Result<(), BindError> {
        let bound = (self.elements.iter()).try_for_each(|element| element.bind(&ui.motion));
        node.modifier = self.elements;
        for setting in &self.settings {
            setting.apply(node, ui);
        }
        bound
    }
}

/// `size`, or the error for its first length that is not finite.
fn finite(size: Size) -> Result<Size, NumberError> {
    let [width, height] = [size.width, size.height].map(FiniteDp::try_new);
    Ok(Size::new(width?.get(), height?.get()))
}

/// A deferred layout value in a modifier element: a length, an offset, a position or a condition.
#[derive(Clone, Copy)]
pub(crate) enum LayoutValue<'a> {
    Length(Axis, &'a Deferred<Dp>),
    Offset(&'a Deferred<Point, Bounds>),
    Position(&'a Deferred<Point, Bounds>),
    Condition(&'a Deferred<bool>),
}

impl<'a> LayoutValue<'a> {
    pub fn used(self) -> Used {
        match self {
            Self::Length(axis, _) => Used::Length(axis),
            Self::Offset(_) => Used::Offset,
            Self::Position(_) => Used::Position,
            Self::Condition(_) => Used::Condition,
        }
    }

    pub fn reads(self) -> &'a [Read] {
        match self {
            Self::Length(_, length) => length.reads(),
            Self::Offset(point) | Self::Position(point) => point.reads(),
            Self::Condition(condition) => condition.reads(),
        }
    }

    /// Binds the animations that this value reads to the clock of `motion`.
    pub fn bind(self, motion: &Motion) -> Result<(), BindError> {
        self.reads().iter().try_for_each(|read| match read {
            Read::Animation(animation) => animation.bind(motion),
            Read::Node(..) => Ok(()),
        })
    }
}

impl ModifierElement {
    /// Binds the animations that this element reads to the clock of `motion`.
    fn bind(&self, motion: &Motion) -> Result<(), BindError> {
        match self {
            Self::AnimatedOffset(animation) => motion.bind(animation)?,
            Self::AnimatedOpacity(animation) => motion.bind(animation)?,
            Self::AnimatedBackground(color) => color.bind(motion)?,
            _ => {}
        }
        self.layout_value()
            .map_or(Ok(()), |value| value.bind(motion))
    }

    pub(crate) fn layout_value(&self) -> Option<LayoutValue<'_>> {
        match self {
            Self::Width(width) => Some(LayoutValue::Length(Axis::Horizontal, width)),
            Self::Height(height) => Some(LayoutValue::Length(Axis::Vertical, height)),
            Self::Offset(by) => Some(LayoutValue::Offset(by)),
            Self::Position(at) => Some(LayoutValue::Position(at)),
            Self::ShownWhen(condition) => Some(LayoutValue::Condition(condition)),
            _ => None,
        }
    }
}

/// What sets the length of a child along the main axis of a row or a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainLength {
    Fills,
    Set,
    Content,
}

impl Node {
    /// The first fill or length along `axis` in the modifier of the node sets the result. A bound,
    /// such as a maximum size, does not count.
    pub(crate) fn main_length(&self, axis: Axis) -> MainLength {
        (self.modifier.iter())
            .find_map(|element| match (axis, element) {
                (Axis::Horizontal, ModifierElement::FillWidth)
                | (Axis::Vertical, ModifierElement::FillHeight) => Some(MainLength::Fills),
                (Axis::Horizontal, ModifierElement::Width(_))
                | (Axis::Vertical, ModifierElement::Height(_)) => Some(MainLength::Set),
                _ => None,
            })
            .unwrap_or(MainLength::Content)
    }
}
