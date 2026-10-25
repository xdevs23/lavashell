// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

pub mod accessibility;
pub mod animation;
pub mod color;
mod compile_time;
pub mod effects;
#[cfg(feature = "serde")]
pub mod encoding;
pub mod error;
pub mod frame;
pub mod geometry;
pub mod gradient;
pub mod image;
pub mod input_method;
pub mod keyboard;
pub mod local;
pub mod numbers;
pub mod output;
pub mod platform;
pub mod preferences;
pub mod protected_text;
pub mod reactive;
pub mod retained;
pub mod scene;
pub mod scroll;
pub mod shape;
mod states;
pub mod style;
pub mod surface;
pub mod transfer;
pub mod ui;
pub mod units;
pub mod view;

mod text_edit;

#[cfg(test)]
mod float_asserts;
#[cfg(test)]
mod test_error;

pub mod prelude {
    pub use crate::accessibility::{
        AccessibilityAction, AccessibilityTree, AccessibleValue, ActionError, ActionKind,
        ActionOutcome, ActionRequest, Checked, CollectionInfo, CollectionItem, LiveRegion,
        RangeInfo, Role, Semantics, SemanticsMode, TextSelection,
    };
    pub use crate::animation::{
        Animatable, Animation, AnimationError, Composite, Contour, Direction, Easing, Keyframe,
        Keyframes, Motion, MotionSpec, Playback, Repeat, Spring, Timer, Trajectory, Tween, Vector,
        VectorSpace,
    };
    pub use crate::color::{ColorIn, ColorInterpolation, Gamut, HuePath, TonalPalette};
    pub use crate::constant;
    pub use crate::cubic_bezier;
    pub use crate::effects::{
        Backdrop, BlendMode, BorderPlacement, Glass, Material, Shadow, ShapedMaterial,
        SourceFilter, SourceFilterId, Stroke,
    };
    pub use crate::frame::{
        FrameClock, FrameMetrics, FrameTime, PresentationTracker, PresentedFrame,
    };
    pub use crate::geometry::{
        Axis, Color, Constraints, ContourPoint, FinitePoint, Insets, Length, PixelSize, PlainPoint,
        Point, Rect, Size, Span, VIEWPORT, Viewport,
    };
    pub use crate::gradient::{ColorStop, Gradient, GradientKind, TileMode};
    pub use crate::image::{
        Anchor, Image, ImageFit, ImageFrame, ImagePlacement, StreamId, StreamImage,
    };
    pub use crate::input_method::{
        Capitalization, FieldPresses, FieldText, SessionId, TextAction, TextCommand, TextHints,
        TextPurpose, TextSender, TextSession,
    };
    pub use crate::keyboard::{Key, Shortcut};
    pub use crate::local::Local;
    pub use crate::output::{HdrMetadata, OutputInfo, OutputProfile, OutputRequest, Precision};
    pub use crate::platform::{ColorScheme, KeyboardPreferences, PlatformFacts};
    pub use crate::preferences::{
        CONTRAST, ContrastPreference, PreferenceOverrides, PreferenceSource, PreferenceValues,
        REDUCED_MOTION, REDUCED_TRANSPARENCY, TEXT_SCALING, TextScaling,
    };
    pub use crate::protected_text::{ProtectedText, SecretText};
    pub use crate::reactive::{Derived, Phase, Scope, Selector, State};
    pub use crate::scene::{
        Affine, Brush, DrawCommand, DrawContext, Drawing, FontSize, FontWeight, Icon, IconPath,
        ParagraphDirection, Segment, TEXT_STYLE, TextAlignment, TextSpan, TextStyle,
        tree::{DamageTracker, Presented, SceneTree, SceneUpdate},
    };
    pub use crate::scroll::{ScrollState, ScrollView, Scrolling};
    pub use crate::shape::{Corners, Polygon, Radius, RoundedRect, Shape, UnionMember};
    pub use crate::spring;
    pub use crate::style::{
        BorderStyle, FocusState, INTERACTION_STYLE, InteractionAppearance, InteractionState,
        InteractionStyle, PointerState, StateStyles, TooltipStyle,
    };
    pub use crate::transfer::{
        DragAction, DragActions, DropAcceptance, ReceivedData, Representation, TransferData,
    };
    pub use crate::ui::{
        Activation, Align, Arrangement, ContentSizes, Deferred, DragInput, DragPhase, IconOptions,
        IconStyle, InputModifiers, Modifier, NodeId, NodeRef, PointerId, PointerInput, PointerKind,
        PointerPhase, TextFieldOptions, TouchId, TransformInput, Ui,
    };
    pub use crate::units::{
        Density, Dp, DpExt, Em, EmExt, FiniteDp, Fraction, NonNegative, NonNegativeDp, NumberError,
        Pct, PctExt, Positive, PositiveDp, Px, PxExt, ScaleFactor, Sp, SpExt, Vh, VhExt, Vw, VwExt,
    };
    pub use crate::view::{
        BoundsMotion, Cursor, InputEvent, Presence, PresenceParts, Proposal, ScaleOrigin,
        TextCaret, TextCluster, TextDropTarget, TextGeometry, TextMeasurer, TextPasteTarget,
        TextSelectionSource, TouchSample, Transition, View,
    };
}
