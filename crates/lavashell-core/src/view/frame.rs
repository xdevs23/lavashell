// SPDX-License-Identifier: GPL-3.0-only
use super::component::Refresh;
use super::frame_error::{FrameError, NodeName};
use super::frame_result::Failures;
use super::input::InputRouting;
use super::paint::PaintOutput;
use super::parents::ROOT;
use super::scroll_followers::Presentation;
use super::{TextMeasurer, View, output, paint};
use crate::frame::FrameTime;
use crate::geometry::{Constraints, Point, VIEWPORT, Viewport};
use crate::keyboard::Shortcut;
use crate::local::{LocalReader, LocalScope};
use crate::reactive::Phase;
use crate::scene::tree::SceneSnapshot;
use crate::scene::{ChangeSink, SceneBuilder, Transform};
use crate::ui::{Action, NodeId, Ui, ViewHandles};
use crate::view::layout::{Layout, Visibility};
use crate::view::moving::Moving;
use std::collections::BTreeSet;

impl View {
    pub fn frame_into<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.stream(sink, |view, sink| {
            view.sync_motion(viewport);
            view.frame_presenting(viewport, text, Presentation::New, sink)
        })
    }

    pub fn probe_scene(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
    ) -> Result<(SceneSnapshot, Failures), FrameError> {
        let needed_frame = self.needs_visual_frame();
        let mut builder = SceneBuilder::default();
        let framed = self.frame_into(viewport, text, &mut builder);
        self.redraw |= needed_frame;
        Ok((builder.snapshot(), framed?))
    }

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
        if !self.painting.shown.is_shown_to(sink.id()) {
            self.painting.restart_sink(sink)?;
        }
        frame(self, sink).or_else(|error| {
            self.invalidate();
            self.painting.restart_sink(sink)?;
            Err(error)
        })
    }

    fn timed_frame<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.sync_motion(viewport);
        self.motion.advance(timing.presentation);
        let presentation = self.followers.advance_presentation(timing.presentation);
        self.frame_presenting(viewport, text, presentation, sink)
    }

    fn sync_motion(&mut self, viewport: Viewport) {
        self.follow_reduced_motion();
        self.motion.set_device_pixel(viewport.pixel());
    }

    fn frame_presenting<S: ChangeSink>(
        &mut self,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        presentation: Presentation,
        sink: &mut S,
    ) -> Result<Failures, FrameError<S::Error>> {
        self.provide(&VIEWPORT, viewport);
        self.finish_input();
        if self.transitions.exits_ended() {
            self.redraw = true;
        }
        self.retire_exits();
        let rested = self.transitions.settle_arrivals();
        self.retained.forget_paintings_around(&rested);
        self.settle_followers(presentation);
        let incoming_focus = self.routing.focus.node();
        let previous_modal = self.routing.modal_root;
        self.failures.start_frame();
        let (modal, painted_focus) = loop {
            self.compose_and_lay_out(text);
            let modal = self.topmost_modal().map(|index| self.nodes[index].id);
            self.routing.modal_root = modal;
            self.sanitize_interactions();
            let painted_focus = self.routing.focus.node();
            self.routing.shortcuts = self.shown_shortcuts();
            self.painting.canvas = output::Canvas::default();
            self.routing.hits.clear();
            self.routing.moving = Moving::default();
            self.followers.clear();
            self.accessibility.geometry.clear();
            let scope = self.scope.clone();
            scope.track(Phase::Paint, || {
                self.crossings = self.crossings_in_flight();
                self.paint(
                    0,
                    Point::default(),
                    Point::default(),
                    Transform::IDENTITY,
                    paint::Inherited::default(),
                    text,
                );
            });
            self.paint_above();
            if !self.fail_paint_failures() {
                break (modal, painted_focus);
            }
        };
        self.transitions.end_first_frame();
        std::mem::swap(&mut self.routing.hits, &mut self.routing.input_hits);
        self.followers.refresh(&mut self.routing.moving);
        self.routing.moving.present(self.motion.now());
        self.sample_accessibility_metadata();
        self.filter_input_hits();
        if modal != previous_modal {
            self.routing
                .move_focus_to_modal(modal, previous_modal, incoming_focus);
        }
        self.normalize_focus();
        self.sanitize_interactions();
        self.withdraw_challengers();
        self.refresh_text_session();
        self.paint_tooltip(text);
        self.redraw = self.routing.focus.node() != painted_focus;
        self.build_accessibility_tree();
        let items = std::mem::take(&mut self.painting.canvas).finish();
        let slots = self.followers.slots();
        self.painting.shown.update(&items, slots, sink)?;
        Ok(self.failures.take())
    }

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

    fn root_composes(&self) -> bool {
        self.nodes.is_empty()
            || (self.scope.dirty().contains(Phase::Composition)
                && !self.failures.failed_in_frame(self.nodes[ROOT].id))
    }

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

    pub(super) fn handles(&self) -> ViewHandles {
        ViewHandles {
            memory: self.memory.clone(),
            motion: self.motion.clone(),
            text_sender: self.editing.sender.clone(),
        }
    }

    pub(super) fn compose_root(&mut self) {
        self.stats.root_compositions += 1;
        self.retained.clear();
        self.failures.layout_retries.clear();
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
        self.routing.root_shortcuts = ui.shortcuts;
        self.replace_tree(ui.nodes);
    }

    fn shown_shortcuts(&self) -> Vec<(Shortcut, Action)> {
        let hidden: BTreeSet<NodeId> = (self.nodes.iter().zip(&self.layout))
            .filter(|(_, layout)| layout.visibility == Visibility::Hidden)
            .map(|(node, _)| node.id)
            .collect();
        let components = (self.nodes.iter()).filter_map(|node| node.component.as_ref());
        let composed = components.flat_map(|component| component.shortcuts.borrow().clone());
        (self.routing.root_shortcuts.iter().cloned())
            .chain(composed)
            .filter(|shortcut| !hidden.contains(&shortcut.node))
            .map(|shortcut| (shortcut.shortcut, shortcut.action))
            .collect()
    }
}

impl PaintOutput {
    fn restart_sink<S: ChangeSink>(&mut self, sink: &mut S) -> Result<(), FrameError<S::Error>> {
        self.shown.retarget(None);
        sink.restart().map_err(FrameError::Sink)?;
        self.shown.retarget(Some(sink.id()));
        Ok(())
    }
}

impl InputRouting {
    fn move_focus_to_modal(
        &mut self,
        modal: Option<NodeId>,
        previous_modal: Option<NodeId>,
        incoming_focus: Option<NodeId>,
    ) {
        let focus = match modal {
            Some(modal) => {
                if previous_modal.is_none() {
                    self.focus.remember_before_modal(incoming_focus);
                }
                self.hits
                    .iter()
                    .find(|h| h.focusable && h.input.is_some())
                    .or_else(|| {
                        self.hits
                            .iter()
                            .find(|h| h.focusable && h.id != modal && h.action.is_some())
                    })
                    .map(|h| h.id)
            }
            None => {
                (self.focus.take_before_modal()).filter(|id| self.hits.iter().any(|h| h.id == *id))
            }
        };
        self.focus.move_to(focus);
    }
}
