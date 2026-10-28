// SPDX-License-Identifier: GPL-3.0-only
use super::component::Refresh;
use super::frame_error::{FrameError, NodeName};
use super::frame_result::{Failures, Frame};
use super::parents::ROOT;
use super::scroll_followers::Presentation;
use super::{
    Action, ApplyError, BTreeSet, Constraints, FrameTime, Layout, LocalReader, LocalScope, Moving,
    NodeId, Phase, Point, SceneTree, SceneUpdate, Shortcut, TextMeasurer, Transform, Ui, VIEWPORT,
    View, ViewHandles, Viewport, Visibility, output, paint,
};

impl View {
    /// Sets `viewport` as the root value of `VIEWPORT`, then composes, lays out and paints what changed. The update of
    /// the returned frame changes the scene made from every earlier update returned by `frame` and `frame_at`. After
    /// an error, and after a frame shown with `show_frame` or `show_frame_at`, the next update builds the whole scene
    /// from an empty scene.
    ///
    /// A component that fails in its composition, its layout or its paint has no nodes in the tree, and the rest of the
    /// view is unchanged. The frame returns the failures since the last frame. A failed component composes again once
    /// a value read by the component changes. A failed root composition leaves the view empty.
    ///
    /// # Errors
    ///
    /// Fails when the update does not fit the scene shown before.
    pub fn frame(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
    ) -> Result<Frame, FrameError> {
        self.shown.retarget(output::Recipient::Caller);
        self.frame_presenting(viewport, text, Presentation::New)
    }

    /// # Errors
    ///
    /// Fails as [`View::frame`] does.
    pub fn frame_at(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<Frame, FrameError> {
        self.shown.retarget(output::Recipient::Caller);
        self.timed_frame(viewport, text, timing)
    }

    /// Makes a frame as [`View::frame`] does, applies the update of the frame to `tree` and returns the failures of
    /// the frame. After an error, `tree` is empty, the next frame sends the whole scene to `tree`, and the next frame
    /// returns the failures.
    ///
    /// # Errors
    ///
    /// Fails as [`View::frame`] does, or when `tree` cannot apply the update of the frame.
    pub fn show_frame(
        &mut self,
        tree: &mut SceneTree,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
    ) -> Result<Failures, FrameError> {
        self.show(tree, |view| {
            view.frame_presenting(viewport, text, Presentation::New)
        })
    }

    /// # Errors
    ///
    /// Fails as [`View::show_frame`] does.
    pub fn show_frame_at(
        &mut self,
        tree: &mut SceneTree,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<Failures, FrameError> {
        self.show(tree, |view| view.timed_frame(viewport, text, timing))
    }

    fn show(
        &mut self,
        tree: &mut SceneTree,
        frame: impl FnOnce(&mut Self) -> Result<Frame, FrameError>,
    ) -> Result<Failures, FrameError> {
        let framed =
            (self.reset_stale_tree(tree).map_err(FrameError::Apply)).and_then(|()| frame(self));
        let shown = match framed {
            Ok(Frame { update, failures }) => match tree.show(update) {
                Ok(()) => Ok(failures),
                Err(error) => {
                    self.failures.carry(failures);
                    Err(FrameError::Apply(error))
                }
            },
            Err(error) => Err(error),
        };
        if shown.is_err() {
            *tree = SceneTree::default();
            self.invalidate();
        }
        shown
    }

    /// Empties `tree` when `tree` did not receive the last frame, so that the next update sends the whole scene to it.
    fn reset_stale_tree(&mut self, tree: &mut SceneTree) -> Result<(), ApplyError> {
        let recipient = output::Recipient::Tree(tree.id());
        if self.shown.is_shown_to(recipient) {
            return Ok(());
        }
        self.shown.retarget(recipient);
        let removal: SceneUpdate = (tree.children(SceneTree::ROOT, None).iter())
            .map(|&id| crate::scene::tree::Change::Remove { id })
            .collect();
        tree.show(removal)
    }

    fn timed_frame(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<Frame, FrameError> {
        self.follow_reduced_motion();
        self.motion.set_device_pixel(viewport.pixel());
        self.motion.advance(timing.presentation);
        let presentation = self.followers.advance_presentation(timing.presentation);
        self.frame_presenting(viewport, text, presentation)
    }

    pub(super) fn frame_presenting(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        presentation: Presentation,
    ) -> Result<Frame, FrameError> {
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
        let update = (self.shown.update(&items, slots)).map_err(FrameError::Apply)?;
        Ok(Frame {
            update,
            failures: self.failures.take(),
        })
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
        let locals = LocalReader::root(LocalScope::root(self.root_values.clone()));
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
