// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod collected;
mod component;
mod crossing;
mod failure;
mod field;
mod focus;
mod frame;
mod frame_error;
mod frame_result;
mod gestures;
mod graph;
mod input;
mod input_method;
mod interaction;
mod key_input;
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
pub(crate) use crossing::Crossing;
use failure::{FrameFailures, LayoutRetry};
pub use frame_error::{
    ComponentError, ComponentFailure, CompositionError, FrameError, LayoutError, LayoutRead,
    LayoutReads, NodeName, NodeValue,
};
pub use frame_result::Failures;
use layout::Layout;
pub use layout::Proposal;
use moving::Moving;
pub use selection::{TextDropTarget, TextPasteTarget, TextSelectionSource};
pub use text_measurer::{TextMeasurer, TextWidths, min_text_width};
use transition::Transitions;
pub use transition::{BoundsMotion, Exit, Transition};

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

use crate::animation::Motion;
use crate::geometry::{Point, Rect, Size, VIEWPORT};
use crate::keyboard::{Key, Shortcut};
use crate::local::RootValues;
use crate::platform::PlatformFacts;
use crate::reactive::{Derived, Phase, Scope};
use crate::scene::Transform;
use crate::shape::{Corners, DrawnRoundedRect, Radius};
use crate::ui::{
    Gestures, InputModifiers, Memory, Node, NodeId, PointerKind, TouchId, Ui, ViewTag,
};
use crate::units::{Density, Dp};
use crate::window::{WINDOW, WindowRegion};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    Scroll { at: Point, delta: Point },
    Text(String),
    Key { key: Key, shift: bool },
    Shortcut(Shortcut),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TouchSample {
    pub id: TouchId,
    pub at: Point,
    pub kind: PointerKind,
}

impl TouchSample {
    #[must_use]
    pub fn finger(id: TouchId, at: Point) -> Self {
        Self {
            id,
            at,
            kind: PointerKind::Finger,
        }
    }
}

#[derive(Clone)]
struct HitArea {
    rect: Rect,
    clips: Vec<HitClip>,
    group: Option<output::GroupId>,
}

impl HitArea {
    fn contains(&self, p: Point, moving: &Moving) -> bool {
        self.rect.contains(moving.painted(self.group, p))
            && (self.clips.iter()).all(|clip| clip.shape.contains(moving.painted(clip.group, p)))
    }
}

#[derive(Clone)]
struct Hit {
    id: NodeId,
    focusable: bool,
    area: HitArea,
    gestures: Gestures,
    input: Option<TextTarget>,
}

impl Hit {
    fn new(id: NodeId, focusable: bool, area: HitArea) -> Self {
        Self {
            id,
            focusable,
            area,
            gestures: Gestures::default(),
            input: None,
        }
    }

    fn use_gestures_of(&mut self, node: &Node) {
        self.gestures.clone_from(&node.gestures);
    }

    fn contains(&self, p: Point, moving: &Moving) -> bool {
        self.area.contains(p, moving)
    }

    fn activates(&self) -> bool {
        self.gestures.activates()
    }
}

#[derive(Clone)]
enum PaintedHit {
    Input(Hit),
    WindowRegion {
        id: NodeId,
        area: HitArea,
        region: WindowRegion,
    },
}

impl PaintedHit {
    fn id(&self) -> NodeId {
        match self {
            Self::Input(hit) => hit.id,
            Self::WindowRegion { id, .. } => *id,
        }
    }

    fn area(&self) -> &HitArea {
        match self {
            Self::Input(hit) => &hit.area,
            Self::WindowRegion { area, .. } => area,
        }
    }
}

struct WindowRegionHit {
    area: HitArea,
    region: WindowRegion,
    input_hits_below: usize,
}

#[derive(Clone, Copy)]
struct TextTarget {
    index: usize,
    origin: Point,
}

impl TextTarget {
    fn new(index: usize, content: Rect) -> Self {
        Self {
            index,
            origin: Point::new(content.x, content.y),
        }
    }

    fn mapped(self, transform: Transform) -> Self {
        Self {
            origin: transform.point(self.origin),
            ..self
        }
    }
}

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
    routing: input::InputRouting,
    editing: field::TextEditing,
    accessibility: accessibility::AccessibilityExport,
    failures: FrameFailures,
    painting: paint::PaintOutput,
    crossings: crossing::Crossings,
    followers: scroll_followers::ScrollFollowers,
    scrolls: BTreeMap<NodeId, Dp>,
    window_focused: bool,
    redraw: bool,
    viewport_origin: Point,
    size: Size,
    memory: Memory,
    motion: Motion,
    transitions: Transitions,
    retained: retained::Retained,
    stats: ViewStats,
    root_values: RootValues,
    viewport_size: Derived<Size>,
    density: Derived<Density>,
    motion_scope: Scope,
}

impl View {
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
            routing: input::InputRouting::default(),
            editing: field::TextEditing::default(),
            accessibility: accessibility::AccessibilityExport::default(),
            failures: FrameFailures::default(),
            painting: paint::PaintOutput::default(),
            crossings: crossing::Crossings::default(),
            followers: scroll_followers::ScrollFollowers::default(),
            scrolls: BTreeMap::new(),
            window_focused: true,
            redraw: true,
            viewport_origin: Point::default(),
            size: Size::ZERO,
            memory: Memory::default(),
            motion: Motion::default(),
            transitions: Transitions::default(),
            retained: retained::Retained::default(),
            stats: ViewStats::default(),
            root_values,
            viewport_size,
            density,
            motion_scope,
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
            || self.motion_scope.is_dirty()
            || self.scope.is_dirty()
            || self.nodes.iter().any(Node::component_is_dirty)
            || self.failures.layout_retries.iter().any(LayoutRetry::is_due)
            || self.motion.moves_read_values()
            || self.motion.timer_due()
            || self.motion.rest_due()
            || !self.editing.sender.is_empty()
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<std::time::Duration> {
        let rest = self.motion.next_rest();
        self.motion.next_deadline().into_iter().chain(rest).min()
    }

    pub fn record_presentation(&mut self, presentation: std::time::Duration) {
        self.routing.moving.present(presentation);
    }

    pub fn invalidate(&mut self) {
        self.redraw = true;
    }

    pub fn set_viewport_origin(&mut self, origin: Point) {
        if self.viewport_origin != origin {
            self.viewport_origin = origin;
            self.redraw = true;
        }
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
    pub fn bounds_of<T: ViewTag>(&self, tag: &T) -> Option<Rect> {
        self.index_tagged(tag).map(|i| self.layout[i].rect)
    }

    fn index_tagged<T: ViewTag>(&self, tag: &T) -> Option<usize> {
        self.nodes.iter().position(|node| node.has_tag(tag))
    }

    #[must_use]
    pub fn window_region_at(&self, at: Point) -> Option<WindowRegion> {
        let window = self.root_values.value(&WINDOW).get()?;
        (self.routing.window_region_at(at))
            .filter(|region| window.offers(region.required_control()))
    }

    #[must_use]
    pub fn focus(&self) -> Option<NodeId> {
        self.routing.focus.node()
    }

    #[must_use]
    pub fn cursor(&self) -> Cursor {
        let pointed =
            (self.routing.primary()).and_then(|primary| primary.capture().or(primary.over));
        self.routing
            .hits
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
