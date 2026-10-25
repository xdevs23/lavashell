// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod collected;
mod component;
mod field;
mod frame;
mod frame_error;
mod gestures;
mod graph;
mod input;
mod input_method;
mod interaction;
mod laid_out;
mod layout;
mod local;
mod motion;
mod moving;
mod navigation;
mod output;
mod paint;
mod parents;
mod place;
mod platform;
mod pointers;
mod protected;
mod reconcile;
mod retained;
mod scroll;
mod scroll_followers;
mod selection;
mod text_measurer;
mod transition;

pub use crate::scene::tree::{Presence, PresenceParts, ScaleOrigin};
pub use crate::text_edit::{TextCaret, TextCluster, TextGeometry};
use component::{collect_descendants, index_of};
pub use frame_error::{
    CompositionError, FrameError, LayoutError, LayoutRead, LayoutReads, NodeName, NodeValue,
};
use interaction::FocusSource;
pub use layout::Proposal;
use layout::{Layout, Measuring};
use moving::Moving;
pub use selection::{TextDropTarget, TextPasteTarget, TextSelectionSource};
pub use text_measurer::{TextMeasurer, TextWidths, min_text_width};
use transition::Transitions;
pub use transition::{BoundsMotion, Transition};

#[cfg(test)]
mod component_tests;
#[cfg(test)]
pub(crate) mod test_scene;
#[cfg(test)]
mod test_tags;
#[cfg(test)]
mod test_text;
#[cfg(test)]
mod tests;

use crate::animation::{AnimationError, Motion};
use crate::frame::FrameTime;
use crate::geometry::{
    Axis, Color, Constraints, Point, Rect, Scale, Size, Span, VIEWPORT, Viewport,
};
use crate::input_method::TextSender;
use crate::keyboard::{Key, Shortcut};
use crate::local::{LocalReader, LocalScope, RootValues};
use crate::platform::PlatformFacts;
use crate::reactive::{Derived, Phase, Scope};
use crate::scene::tree::{ApplyError, SceneTree, SceneUpdate};
use crate::scene::{Affine, Brush, DrawCommand, Drawing, TextStyle, Transform};
use crate::shape::{Corners, DrawnRoundedRect, Radius};
use crate::text_edit::{FieldGeometry, TextEdit};
use crate::ui::{
    Action, ActivateAction, Activation, Arrangement, Deferred, DragInput, DragPhase, Draggable,
    IconSize, InputModifiers, Memory, ModifierElement, Node, NodeId, NodeKind, NodeRef,
    PointerAction, PointerId, PointerInput, PointerKind, PointerPhase, TouchId, TransformHandler,
    TransformInput, Ui, ViewHandles, Weight,
};
use crate::units::{Density, Dp};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
}

pub enum InputEvent {
    Leave,
    CancelPointer,
    WindowFocus(bool),
    KeyRelease(Key),
    Modifiers(InputModifiers),
    Context(Point),
    Move(Point),
    Press(Point),
    Release(Point),
    TouchDown(TouchSample),
    TouchMove(TouchSample),
    TouchUp(TouchSample),
    TouchCancel(TouchId),
    /// A scroll wheel or touchpad scrolled by `delta` along each axis while the pointer was at `at`. A positive
    /// component of `delta` scrolls toward the right or the bottom of the content.
    Scroll {
        at: Point,
        delta: Point,
    },
    Text(String),
    Key {
        key: Key,
        shift: bool,
    },
    Shortcut(Shortcut),
}

/// One position of a touch point, reported by the host with the id of the touch and the kind of pointer that
/// made it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchSample {
    pub id: TouchId,
    pub at: Point,
    pub kind: PointerKind,
}

/// An area of a node that receives pointer input, with the handlers of that node. `rect` and `clips` are in the
/// coordinates of the view as painted, inside the moving group `group`.
#[derive(Clone)]
struct Hit {
    id: NodeId,
    focusable: bool,
    rect: Rect,
    clips: Vec<HitClip>,
    group: Option<output::GroupId>,
    action: Option<Action>,
    pointer: Option<PointerAction>,
    drag: Option<Draggable>,
    transform: Option<TransformHandler>,
    input: Option<usize>,
    activate: Option<ActivateAction>,
    context: Option<ActivateAction>,
}

impl Hit {
    fn new(id: NodeId, focusable: bool, rect: Rect, clips: Vec<HitClip>) -> Self {
        Self {
            id,
            focusable,
            rect,
            clips,
            group: None,
            action: None,
            pointer: None,
            drag: None,
            transform: None,
            input: None,
            activate: None,
            context: None,
        }
    }

    fn contains(&self, p: Point, moving: &Moving) -> bool {
        self.rect.contains(moving.painted(self.group, p))
            && (self.clips.iter()).all(|clip| clip.shape.contains(moving.painted(clip.group, p)))
    }

    /// Whether a click or the keyboard activates the node of the hit.
    fn activates(&self) -> bool {
        self.action.is_some() || self.activate.is_some()
    }
}

/// A rounded rectangle that clips a [`Hit`]. `shape` has the corners as drawn and is in the coordinates of the view
/// as painted, inside the moving group `group`.
#[derive(Clone, Copy)]
struct HitClip {
    shape: DrawnRoundedRect,
    group: Option<output::GroupId>,
}

impl HitClip {
    fn new(rect: Rect, corners: impl Into<Corners<Radius>>, transform: Transform) -> Self {
        let scaled = corners
            .into()
            .map(|r| Radius::new(r.x * transform.scale.x.abs(), r.y * transform.scale.y.abs()));
        Self {
            shape: DrawnRoundedRect::new(transform.rect(rect), scaled),
            group: None,
        }
    }

    fn in_group(self, group: Option<output::GroupId>) -> Self {
        Self { group, ..self }
    }

    fn mapped(self, transform: Transform) -> Self {
        let mapped = Self::new(self.shape.rect(), self.shape.corners(), transform);
        mapped.in_group(self.group)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewStats {
    pub root_compositions: u64,
    pub component_compositions: u64,
    pub painted_components: u64,
    pub reused_components: u64,
    pub measured_components: u64,
    pub reused_layouts: u64,
}

pub struct View {
    compose: Box<dyn FnMut(&mut Ui)>,
    scope: Scope,
    nodes: Vec<Node>,
    parents: parents::Parents,
    graph: graph::Graph,
    layout: Vec<Layout>,
    hits: Vec<Hit>,
    input_hits: Vec<Hit>,
    moving: Moving,
    followers: scroll_followers::ScrollFollowers,
    canvas: output::Canvas,
    spare_unwinds: Vec<paint::Unwind>,
    shown: output::Shown,
    scrolls: BTreeMap<NodeId, Dp>,
    focus: Option<NodeId>,
    focus_source: Option<FocusSource>,
    modifiers: InputModifiers,
    modal_root: Option<NodeId>,
    modal_restore: Option<NodeId>,
    pointers: BTreeMap<PointerId, pointers::Pointer>,
    keyboard_press: Option<(NodeId, Key)>,
    window_focused: bool,
    interaction_visuals: BTreeMap<NodeId, interaction::StyledAppearance>,
    redraw: bool,
    size: Size,
    memory: Memory,
    shortcuts: Vec<(Shortcut, Action)>,
    root_shortcuts: Vec<(Shortcut, Action)>,
    edits: BTreeMap<NodeId, TextEdit>,
    text_positions: BTreeMap<NodeId, FieldGeometry>,
    sessions: input_method::Sessions,
    text_sender: TextSender,
    motion: Motion,
    transitions: Transitions,
    sibling_orders: paint::SiblingOrders,
    retained: retained::Retained,
    stats: ViewStats,
    accessibility: crate::accessibility::AccessibilityTree,
    accessibility_geometry: BTreeMap<NodeId, accessibility::SemanticGeometry>,
    accessibility_text_geometry: bool,
    accessibility_scope: Scope,
    accessibility_metadata: BTreeMap<NodeId, crate::accessibility::Semantics>,
    root_values: RootValues,
    /// The size of the root viewport. Code that reads it reruns when the size changes. A change of the density alone
    /// does not rerun that code.
    viewport_size: Derived<Size>,
    /// The density of the root viewport. Code that reads it reruns only when the density changes.
    density: Derived<Density>,
    /// Records the read of the reduced motion preference, so that the view passes each change of the preference to
    /// `motion`.
    motion_scope: Scope,
    /// The error of the first animation, started by a transition or a state style while the frame painted, whose
    /// value or velocity was not finite. `frame_at` returns this error in place of the frame.
    paint_failure: Option<AnimationError>,
}

impl View {
    /// A view that calls `compose` to build its nodes and reads the settings of the platform, such as the touch slop
    /// and the caret width, from `platform`.
    pub fn new(platform: PlatformFacts, compose: impl FnMut(&mut Ui) + 'static) -> Self {
        let motion_scope = Scope::default();
        motion_scope.invalidate(Phase::Composition);
        let root_values = RootValues::new(platform);
        let viewport = root_values.value(&VIEWPORT);
        let viewport_size = {
            let viewport = viewport.clone();
            Derived::new(move || viewport.with(|viewport| viewport.size))
        };
        let density = Derived::new(move || viewport.with(|viewport| viewport.density));
        Self {
            compose: Box::new(compose),
            scope: Scope::default(),
            nodes: Vec::new(),
            parents: parents::Parents::default(),
            graph: graph::Graph::default(),
            layout: Vec::new(),
            hits: Vec::new(),
            input_hits: Vec::new(),
            moving: Moving::default(),
            followers: scroll_followers::ScrollFollowers::default(),
            canvas: output::Canvas::default(),
            spare_unwinds: Vec::new(),
            shown: output::Shown::default(),
            scrolls: BTreeMap::new(),
            focus: None,
            focus_source: None,
            modifiers: InputModifiers::default(),
            modal_root: None,
            modal_restore: None,
            pointers: BTreeMap::new(),
            keyboard_press: None,
            window_focused: true,
            interaction_visuals: BTreeMap::new(),
            redraw: true,
            size: Size::ZERO,
            memory: Memory::default(),
            shortcuts: Vec::new(),
            root_shortcuts: Vec::new(),
            edits: BTreeMap::new(),
            text_positions: BTreeMap::new(),
            sessions: input_method::Sessions::default(),
            text_sender: TextSender::new(),
            motion: Motion::default(),
            transitions: Transitions::default(),
            sibling_orders: paint::SiblingOrders::default(),
            retained: retained::Retained::default(),
            stats: ViewStats::default(),
            accessibility: crate::accessibility::AccessibilityTree::default(),
            accessibility_geometry: BTreeMap::new(),
            accessibility_text_geometry: false,
            accessibility_scope: Scope::default(),
            accessibility_metadata: BTreeMap::new(),
            root_values,
            viewport_size,
            density,
            motion_scope,
            paint_failure: None,
        }
    }

    #[must_use]
    pub fn needs_frame(&self) -> bool {
        self.needs_visual_frame() || self.needs_accessibility_update()
    }

    #[must_use]
    pub fn needs_visual_frame(&self) -> bool {
        self.redraw
            || self.followers.needs_frame()
            || self.motion_scope.dirty().any()
            || self.scope.dirty().any()
            || self
                .nodes
                .iter()
                .any(|n| n.component.as_ref().is_some_and(|c| c.scope.dirty().any()))
            || self.motion.moves_read_values()
            || self.motion.timer_due()
            || self.motion.rest_due()
            || !self.text_sender.is_empty()
    }

    /// The earliest time at which a timer fires or a running animation comes to rest, measured from the same start as
    /// the presentation times passed to `frame_at`. A host that sleeps between frames needs to wake at that time.
    #[must_use]
    pub fn next_deadline(&self) -> Option<std::time::Duration> {
        let rest = self.motion.next_rest();
        self.motion.next_deadline().into_iter().chain(rest).min()
    }

    /// Records the time at which the consumer showed the scene. Pointer input then finds each node in a moving group
    /// where the animated transform of the group placed it at that time.
    pub fn record_presentation(&mut self, presentation: std::time::Duration) {
        self.moving.present(presentation);
    }

    pub fn invalidate(&mut self) {
        self.redraw = true;
    }

    #[must_use]
    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    #[must_use]
    pub fn stats(&self) -> ViewStats {
        self.stats
    }

    #[must_use]
    pub fn bounds_of(&self, tag: &str) -> Option<Rect> {
        self.nodes
            .iter()
            .position(|n| n.tag.as_deref() == Some(tag))
            .map(|i| self.layout[i].rect)
    }

    #[must_use]
    pub fn focus(&self) -> Option<NodeId> {
        self.focus
    }

    #[must_use]
    pub fn cursor(&self) -> Cursor {
        let pointed = (self.primary()).and_then(|primary| primary.capture().or(primary.over));
        self.hits
            .iter()
            .rev()
            .find(|hit| Some(hit.id) == pointed)
            .map_or(Cursor::Default, |hit| {
                if hit.input.is_some() {
                    Cursor::Text
                } else {
                    Cursor::Pointer
                }
            })
    }
}
