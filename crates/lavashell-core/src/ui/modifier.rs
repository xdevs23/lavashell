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
use crate::view::CompositionError;
use std::rc::Rc;

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
    Invalid {
        value: ModifierValue,
        cause: NumberError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierValue {
    MeasurementLimit,
    MinimumSize,
    MaximumSize,
    Gap,
    BorderWidth,
    Blur,
    OutlineWidth,
    MaterialGroupSpacing,
    Opacity,
}

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

    pub fn measurement_limit(self, limit: Size) -> Self {
        let limit = non_negative(limit);
        self.checked(
            ModifierValue::MeasurementLimit,
            limit,
            ModifierElement::MeasurementLimit,
        )
    }

    pub fn width(self, value: impl Into<Deferred<Dp>>) -> Self {
        self.push(ModifierElement::Width(value.into()))
    }

    pub fn height(self, value: impl Into<Deferred<Dp>>) -> Self {
        self.push(ModifierElement::Height(value.into()))
    }

    pub fn size(self, width: impl Into<Deferred<Dp>>, height: impl Into<Deferred<Dp>>) -> Self {
        self.width(width).height(height)
    }

    pub fn min_size(self, width: Dp, height: Dp) -> Self {
        let size = finite(Size::new(width, height));
        self.checked(ModifierValue::MinimumSize, size, ModifierElement::MinSize)
    }

    pub fn max_width(self, width: Dp) -> Self {
        self.max_size(Size::new(width, Dp::INFINITY))
    }

    pub fn max_height(self, height: Dp) -> Self {
        self.max_size(Size::new(Dp::INFINITY, height))
    }

    pub fn size_in(self, min: Size, max: Size) -> Self {
        self.max_size(max).min_size(min.width, min.height)
    }

    fn max_size(self, max: Size) -> Self {
        let max = unbounded_or_non_negative(max);
        self.checked(ModifierValue::MaximumSize, max, ModifierElement::MaxSize)
    }

    pub fn fill_max_width(self) -> Self {
        self.push(ModifierElement::FillWidth)
    }

    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillHeight)
    }

    pub fn background(self, color: impl Into<Brush>) -> Self {
        self.push(ModifierElement::Background(color.into()))
    }

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

    pub fn clip_rect(self, rect: Rect, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::ClipRect(rect, corners.into()))
    }

    pub fn blur(self, sigma: Dp) -> Self {
        let blur = NonNegativeDp::try_new(sigma);
        self.checked(ModifierValue::Blur, blur, |blur| ModifierElement::Effect {
            blur,
            filter: None,
        })
    }

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

    pub fn material_group(self, material: Material, spacing: Dp) -> Self {
        let spacing = NonNegativeDp::try_new(spacing);
        self.checked(ModifierValue::MaterialGroupSpacing, spacing, |spacing| {
            ModifierElement::MaterialGroup { material, spacing }
        })
    }

    pub fn island(self, corners: impl Into<Corners>) -> Self {
        self.push(ModifierElement::Island(corners.into()))
    }

    pub fn opacity(self, value: f64) -> Self {
        let opacity = Fraction::try_new(value);
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

    pub fn position(self, at: impl Into<Deferred<Point, Bounds>>) -> Self {
        self.push(ModifierElement::Position(at.into()))
    }

    pub fn shown_when(self, condition: impl Into<Deferred<bool>>) -> Self {
        self.push(ModifierElement::ShownWhen(condition.into()))
    }

    pub fn animated_offset(self, value: Animation<Point>) -> Self {
        self.push(ModifierElement::AnimatedOffset(value))
    }

    pub fn animated_opacity(self, value: Animation<f64>) -> Self {
        self.push(ModifierElement::AnimatedOpacity(value))
    }

    pub fn follow_scroll(self, scroll: ScrollState, travel: Dp) -> Self {
        self.push(ModifierElement::FollowScroll { scroll, travel })
    }

    pub fn animated_background(self, value: impl AnimatedColor + 'static) -> Self {
        self.push(ModifierElement::AnimatedBackground(Rc::new(value)))
    }

    pub fn then(mut self, other: Self) -> Self {
        self.elements.extend(other.elements);
        self.settings.extend(other.settings);
        self
    }

    pub(super) fn apply(self, node: &mut Node, ui: &Ui) -> Result<(), CompositionError> {
        let bound = (self.elements.iter()).try_for_each(|element| element.bind(&ui.motion));
        node.modifier = self.elements;
        (self.settings.iter()).try_for_each(|setting| setting.apply(node, ui))?;
        Ok(bound?)
    }
}

fn finite(size: Size) -> Result<Size, NumberError> {
    let [width, height] = [size.width, size.height].map(FiniteDp::try_new);
    Ok(Size::new(width?.get(), height?.get()))
}

fn non_negative(size: Size) -> Result<Size, NumberError> {
    let [width, height] = [size.width, size.height].map(NonNegativeDp::try_new);
    Ok(Size::new(width?.get(), height?.get()))
}

fn unbounded_or_non_negative(size: Size) -> Result<Size, NumberError> {
    let [width, height] = [size.width, size.height].map(|length| {
        if length == Dp::INFINITY {
            Ok(length)
        } else {
            NonNegativeDp::try_new(length).map(NonNegativeDp::get)
        }
    });
    Ok(Size::new(width?, height?))
}

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

    pub fn bind(self, motion: &Motion) -> Result<(), BindError> {
        self.reads().iter().try_for_each(|read| match read {
            Read::Animation(animation) => animation.bind(motion),
            Read::Node(..) => Ok(()),
        })
    }
}

impl ModifierElement {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainLength {
    Fills,
    Set,
    Content,
}

impl Node {
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
