// SPDX-License-Identifier: GPL-3.0-only
use super::component::Refresh;
use super::frame_error::{FrameError, LayoutError, NodeName};
use super::scroll_followers::Presentation;
use super::{
    ApplyError, Constraints, FrameTime, Layout, LocalReader, LocalScope, Moving, NodeId, Phase,
    Point, SceneTree, SceneUpdate, TextMeasurer, Transform, Ui, VIEWPORT, View, ViewHandles,
    Viewport, output, paint,
};
use crate::animation::AnimationError;

impl View {
    /// Sets `viewport` as the root value of `VIEWPORT`, then composes, lays out and paints what changed. The returned
    /// update changes the scene made from every earlier update returned by `frame` and `frame_at`. After an error, and
    /// after a frame shown with `show_frame` or `show_frame_at`, the next update builds the whole scene from an empty
    /// scene.
    ///
    /// # Errors
    ///
    /// Fails when the composed tree cannot be laid out, when each layout makes a component compose again in a cycle,
    /// when an animation painted by the frame has a value or velocity that is not finite, or when the update does not
    /// fit the scene shown before.
    pub fn frame(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
    ) -> Result<SceneUpdate, FrameError> {
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
    ) -> Result<SceneUpdate, FrameError> {
        self.shown.retarget(output::Recipient::Caller);
        self.timed_frame(viewport, text, timing)
    }

    /// Makes a frame as [`View::frame`] does and applies the update of the frame to `tree`. After an error, `tree` is
    /// empty, and the next frame sends the whole scene to `tree`.
    ///
    /// # Errors
    ///
    /// Fails as [`View::frame`] does, or when `tree` cannot apply the update of the frame.
    pub fn show_frame(
        &mut self,
        tree: &mut SceneTree,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
    ) -> Result<(), FrameError> {
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
    ) -> Result<(), FrameError> {
        self.show(tree, |view| view.timed_frame(viewport, text, timing))
    }

    fn show(
        &mut self,
        tree: &mut SceneTree,
        frame: impl FnOnce(&mut Self) -> Result<SceneUpdate, FrameError>,
    ) -> Result<(), FrameError> {
        let shown = (self.reset_stale_tree(tree).map_err(FrameError::Apply))
            .and_then(|()| frame(self))
            .and_then(|update| tree.show(update).map_err(FrameError::Apply));
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
    ) -> Result<SceneUpdate, FrameError> {
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
    ) -> Result<SceneUpdate, FrameError> {
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
        self.compose_and_lay_out(text)?;
        let previous_modal = self.modal_root;
        let modal = self.nodes.iter().rev().find(|n| n.modal).map(|n| n.id);
        self.modal_root = modal;
        self.sanitize_interactions();
        let painted_focus = self.focus;
        self.shortcuts = self.root_shortcuts.clone();
        for node in &self.nodes {
            if let Some(component) = &node.component {
                self.shortcuts
                    .extend(component.shortcuts.borrow().iter().cloned());
            }
        }
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
        self.refresh_text_session();
        self.paint_tooltip(text);
        self.redraw = self.focus != painted_focus;
        self.build_accessibility_tree();
        if let Some(error) = self.paint_failure.take() {
            return Err(FrameError::Paint(error));
        }
        let items = std::mem::take(&mut self.canvas).finish();
        (self.shown.update(&items, self.followers.slots())).map_err(FrameError::Apply)
    }

    /// Records `error` as the error of the frame being painted, unless the same paint already recorded an earlier
    /// error. The frame fails with the first error recorded.
    pub(super) fn fail_paint(&mut self, error: AnimationError) {
        self.paint_failure = self.paint_failure.or(Some(error));
    }

    /// Composes the dirty parts of the tree and lays the tree out, and repeats both steps while a layout makes a
    /// component compose again. A composition that is dirty again after a round that produced the same nodes and
    /// layout as an earlier round would repeat forever. This function returns that case as a cycle.
    fn compose_and_lay_out(&mut self, text: &mut impl TextMeasurer) -> Result<(), FrameError> {
        let mut laid_out: Vec<(Vec<NodeId>, Vec<Layout>)> = Vec::new();
        loop {
            if let Err(error) = self.compose_and_lay_out_once(text) {
                self.scope.invalidate(Phase::Composition);
                return Err(FrameError::Layout(error));
            }
            let Some(dirty) = self.dirty_composition() else {
                return Ok(());
            };
            let layout = (
                self.nodes.iter().map(|node| node.id).collect(),
                self.layout.clone(),
            );
            if laid_out.contains(&layout) {
                let node = NodeName::of(&self.nodes[dirty]);
                return Err(FrameError::CompositionCycle { node });
            }
            laid_out.push(layout);
        }
    }

    fn compose_and_lay_out_once(
        &mut self,
        text: &mut impl TextMeasurer,
    ) -> Result<(), LayoutError> {
        let recompose = self.nodes.is_empty() || self.scope.dirty().contains(Phase::Composition);
        if recompose {
            self.compose_root()?;
        }
        let scoped_changed = !recompose && self.refresh_components()? == Refresh::Recomposed;
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
            let scope = self.scope.clone();
            let viewport_size = self.viewport_size.clone();
            scope.track(Phase::Layout, || {
                self.lay_out(Constraints::tight(viewport_size.get()), text)
            })?;
        }
        Ok(())
    }

    /// The index of a node whose composition is dirty: the root when the root composition is dirty, or else the first
    /// component with a dirty composition.
    fn dirty_composition(&self) -> Option<usize> {
        if self.scope.dirty().contains(Phase::Composition) {
            return Some(0);
        }
        self.nodes.iter().position(|node| {
            (node.component.as_ref())
                .is_some_and(|component| component.scope.dirty().contains(Phase::Composition))
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

    pub(super) fn compose_root(&mut self) -> Result<(), LayoutError> {
        self.stats.root_compositions += 1;
        self.retained.clear();
        let handles = self.handles();
        let compose = &mut self.compose;
        let locals = LocalReader::root(LocalScope::root(self.root_values.clone()));
        let ui = self.scope.track(Phase::Composition, || {
            let mut ui = Ui::composing(handles, locals);
            compose(&mut ui);
            ui
        });
        if let Some(error) = ui.failure {
            return Err(LayoutError::Composition(error));
        }
        self.reconcile(ui.nodes)?;
        self.root_shortcuts = ui.shortcuts;
        Ok(())
    }
}
