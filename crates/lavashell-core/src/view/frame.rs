// SPDX-License-Identifier: GPL-3.0-only
use super::component::Refresh;
use super::frame_error::{FrameError, NodeName};
use super::frame_result::Failures;
use super::parents::ROOT;
use super::scroll_followers::Presentation;
use super::{
    Action, BTreeSet, Constraints, FrameTime, Layout, LocalReader, LocalScope, Moving, NodeId,
    Phase, Point, Shortcut, TextMeasurer, Transform, Ui, VIEWPORT, View, ViewHandles, Viewport,
    Visibility, output, paint,
};
use crate::scene::ChangeSink;

impl View {
    /// Sets `viewport` as the root value of `VIEWPORT`, then composes, lays out and paints what changed. Sends each
    /// change to `sink` while the view composes the frame, and then ends the frame in `sink`. The changes update the
    /// scene that `sink` built from earlier frames. `sink` restarts first when it did not receive the last frame, or
    /// when the last restart of `sink` failed. An error restarts `sink` at once. The frame after a restart sends the
    /// whole scene.
    ///
    /// A component that fails in its composition, its layout or its paint has no nodes in the tree, and the rest of the
    /// view is unchanged. Returns the failures since the last frame. A failed component composes again once a value
    /// read by the component changes. A failed root composition leaves the view empty.
    ///
    /// # Errors
    ///
    /// Fails when a change does not fit the scene shown before, or when `sink` fails on a change, on the end of the
    /// frame or on a restart. A failed restart replaces the error that caused it.
    pub fn frame_into<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.stream(sink, |view, sink| {
            view.frame_presenting(viewport, text, Presentation::New, sink)
        })
    }

    /// Advances the motions of the view to the presentation time of `timing`, then makes a frame as
    /// [`View::frame_into`] does.
    ///
    /// # Errors
    ///
    /// Fails as [`View::frame_into`] does.
    pub fn frame_into_at<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.stream(sink, |view, sink| {
            view.timed_frame(viewport, text, timing, sink)
        })
    }

    fn stream<S: ChangeSink>(
        &mut self,
        sink: &mut S,
        frame: impl FnOnce(&mut Self, &mut S) -> Result<Failures, FrameError<S::Error>>,
    ) -> Result<Failures, FrameError<S::Error>> {
        if !self.shown.is_shown_to(sink.id()) {
            self.restart_sink(sink)?;
        }
        frame(self, sink).or_else(|error| {
            self.invalidate();
            self.restart_sink(sink)?;
            Err(error)
        })
    }

    /// Restarts `sink`, so that the next frame sends the whole scene to it. Until a restart succeeds, every frame into
    /// `sink` restarts it first.
    fn restart_sink<S: ChangeSink>(&mut self, sink: &mut S) -> Result<(), FrameError<S::Error>> {
        self.shown.retarget(None);
        sink.restart().map_err(FrameError::Sink)?;
        self.shown.retarget(Some(sink.id()));
        Ok(())
    }

    fn timed_frame<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.follow_reduced_motion();
        self.motion.set_device_pixel(viewport.pixel());
        self.motion.advance(timing.presentation);
        let presentation = self.followers.advance_presentation(timing.presentation);
        self.frame_presenting(viewport, text, presentation, sink)
    }

    fn frame_presenting<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        presentation: Presentation,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.provide(&VIEWPORT, viewport);
        self.motion.set_device_pixel(viewport.pixel());
        self.finish_input();
        self.follow_reduced_motion();
        if self.transitions.exits_ended() {
            self.redraw = true;
        }
        self.retire_exits();
        self.settle_followers(presentation);
        let incoming_focus = self.focus;
        let previous_modal = self.modal_root;
        self.failures.start_frame();
        // A component that fails in the paint loses its nodes, so the frame lays out and paints again.
        let (modal, painted_focus) = loop {
            self.compose_and_lay_out(text);
            let modal = self.topmost_modal().map(|index| self.nodes[index].id);
            self.modal_root = modal;
            self.sanitize_interactions();
            let painted_focus = self.focus;
            self.shortcuts = self.shown_shortcuts();
            self.canvas = output::Canvas::default();
            self.hits.clear();
            self.moving = Moving::default();
            self.followers.clear();
            self.accessibility_geometry.clear();
            let scope = self.scope.clone();
            scope.track(Phase::Paint, || {
                self.paint(
                    0,
                    Point::default(),
                    Point::default(),
                    Transform::IDENTITY,
                    paint::Inherited::default(),
                    text,
                );
            });
            if !self.fail_paint_failures() {
                break (modal, painted_focus);
            }
        };
        std::mem::swap(&mut self.hits, &mut self.input_hits);
        self.followers.refresh(&mut self.moving);
        self.moving.present(self.motion.now());
        self.sample_accessibility_metadata();
        self.filter_input_hits();
        if modal != previous_modal {
            self.move_focus_to_modal(modal, previous_modal, incoming_focus);
        }
        self.normalize_focus();
        self.sanitize_interactions();
        self.withdraw_challengers();
        self.refresh_text_session();
        self.paint_tooltip(text);
        self.redraw = self.focus != painted_focus;
        self.build_accessibility_tree();
        let items = std::mem::take(&mut self.canvas).finish();
        let slots = self.followers.slots();
        // The failures of a frame whose update fails stay for the next frame.
        self.shown.update(&items, slots, sink)?;
        Ok(self.failures.take())
    }

    /// Composes the dirty parts of the tree and lays the tree out, and repeats both steps while a layout makes a
    /// composition compose again. A composition that is dirty again after a round that produced the same nodes and
    /// layout as an earlier round would repeat forever. That composition fails.
    fn compose_and_lay_out(&mut self, text: &mut impl TextMeasurer) {
        let mut laid_out: Vec<(Vec<NodeId>, Vec<Layout>)> = Vec::new();
        loop {
            self.compose_and_lay_out_once(text);
            let Some(dirty) = self.dirty_composition() else {
                self.settle_cycles();
                return;
            };
            let layout = (
                self.nodes.iter().map(|node| node.id).collect(),
                self.layout.clone(),
            );
            if laid_out.contains(&layout) {
                self.fail_cycle(dirty);
            } else {
                laid_out.push(layout);
            }
        }
    }

    fn compose_and_lay_out_once(&mut self, text: &mut impl TextMeasurer) {
        self.retry_changed_layouts();
        let recompose = self.root_composes();
        if recompose {
            self.compose_root();
        }
        let scoped_changed = !recompose && self.refresh_components() == Refresh::Recomposed;
        let scoped_layout = self.nodes.iter().any(|n| {
            n.component
                .as_ref()
                .is_some_and(|c| c.scope.dirty().contains(Phase::Layout))
        });
        if recompose
            || scoped_changed
            || scoped_layout
            || self.redraw
            || self.scope.dirty().contains(Phase::Layout)
        {
            self.lay_out_tree(text);
        }
    }

    /// Lays the tree out. A scroll state that cannot scroll fails the composition that composed its node, and the tree
    /// is laid out again without the contents of that composition.
    fn lay_out_tree(&mut self, text: &mut impl TextMeasurer) {
        loop {
            let scope = self.scope.clone();
            let viewport_size = self.viewport_size.clone();
            let failed = scope.track(Phase::Layout, || {
                self.lay_out(Constraints::tight(viewport_size.get()), text)
            });
            if !self.fail(failed) {
                return;
            }
        }
    }

    /// Whether the root composition composes in this round: before the first frame, and when it is dirty and did not
    /// fail in this frame.
    fn root_composes(&self) -> bool {
        self.nodes.is_empty()
            || (self.scope.dirty().contains(Phase::Composition)
                && !self.failures.failed_in_frame(self.nodes[ROOT].id))
    }

    /// The index of a node whose composition is dirty and did not fail in this frame: the root when the root
    /// composition is such, or else the first such component.
    fn dirty_composition(&self) -> Option<usize> {
        if self.root_composes() {
            return Some(ROOT);
        }
        self.nodes.iter().position(|node| {
            (node.component.as_ref())
                .is_some_and(|component| component.scope.dirty().contains(Phase::Composition))
                && !self.failures.failed_in_frame(node.id)
        })
    }

    /// Moves the focus into a modal node that appeared. When the modal node left, moves the focus back to the node
    /// focused before the modal node appeared, if that node still receives input.
    fn move_focus_to_modal(
        &mut self,
        modal: Option<NodeId>,
        previous_modal: Option<NodeId>,
        incoming_focus: Option<NodeId>,
    ) {
        match modal {
            Some(modal) => {
                if previous_modal.is_none() {
                    self.modal_restore = incoming_focus;
                }
                self.focus = self
                    .hits
                    .iter()
                    .find(|h| h.focusable && h.input.is_some())
                    .or_else(|| {
                        self.hits
                            .iter()
                            .find(|h| h.focusable && h.id != modal && h.action.is_some())
                    })
                    .map(|h| h.id);
            }
            None => {
                self.focus = self
                    .modal_restore
                    .take()
                    .filter(|id| self.hits.iter().any(|h| h.id == *id));
            }
        }
    }

    pub(super) fn handles(&self) -> ViewHandles {
        ViewHandles {
            memory: self.memory.clone(),
            motion: self.motion.clone(),
            text_sender: self.text_sender.clone(),
        }
    }

    /// Composes the root composition and inserts the nodes built by the root composition into the tree. A failed root
    /// composition leaves only the root node in the tree.
    pub(super) fn compose_root(&mut self) {
        self.stats.root_compositions += 1;
        self.retained.clear();
        self.layout_retries.clear();
        let handles = self.handles();
        let compose = &mut self.compose;
        let locals = LocalReader::new(LocalScope::root(self.root_values.clone()));
        let mut ui = self.scope.track(Phase::Composition, || {
            let mut ui = Ui::composing(handles, locals);
            compose(&mut ui);
            ui
        });
        let root = NodeName::of(&ui.nodes[ROOT]);
        ui.record_failure(root);
        self.failures.extend(ui.failures);
        self.root_shortcuts = ui.shortcuts;
        self.replace_tree(ui.nodes);
    }

    /// The shortcuts of the root composition and of each component, apart from those composed in a hidden node.
    fn shown_shortcuts(&self) -> Vec<(Shortcut, Action)> {
        let hidden: BTreeSet<NodeId> = (self.nodes.iter().zip(&self.layout))
            .filter(|(_, layout)| layout.visibility == Visibility::Hidden)
            .map(|(node, _)| node.id)
            .collect();
        let components = (self.nodes.iter()).filter_map(|node| node.component.as_ref());
        let composed = components.flat_map(|component| component.shortcuts.borrow().clone());
        (self.root_shortcuts.iter().cloned())
            .chain(composed)
            .filter(|shortcut| !hidden.contains(&shortcut.node))
            .map(|shortcut| (shortcut.shortcut, shortcut.action))
            .collect()
    }
}
