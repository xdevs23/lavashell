// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::{Clipboard, ClipboardTransfers};
use crate::drag::{DragGesture, DragRequest, Drags, DropEvent};
use crate::layer_surface::LayerSurface;
use crate::observers::{HostKey, KeyboardFocus, Observers};
use crate::panels::PanelsPerOutput;
use crate::run::{RuntimeSetup, SurfaceSetup};
use crate::run_end::RunEnd;
use crate::session_lock::{LockConfiguration, SessionLock};
use crate::view_actions::{ActionQueue, ViewActions};
use crate::watches::Watches;
use crate::window::Window;
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::Viewport;
use lavashell_core::output::{MonitorInfo, OutputInfo};
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::reactive::State;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragEvent, DropAcceptance};
use lavashell_core::ui::Ui;
use lavashell_core::view::{ComponentFailure, InputEvent, View};
use lavashell_renderer::vulkan::SceneSource;
use lavashell_renderer::{Fonts, Painter, RendererSetup};
use std::time::Duration;

mod error;

pub use error::SetupError;

/// The surfaces, the observer callbacks and the renderer setup of an app. [`run`](crate::run) runs it on a platform.
pub struct App {
    setup: RuntimeSetup,
    platform: PlatformFacts,
}

impl App {
    /// Sets `filter`, which receives each keyboard input event of a surface after its keys are resolved and before
    /// text editing, selection and clipboard transfers handle it. The filter returns the event to pass on, or `None`
    /// to consume it. A consumed event is never passed to the view, so text typed with it appears neither in the
    /// nodes of the view nor in the accessibility tree.
    #[must_use]
    pub fn input_filter(
        mut self,
        filter: impl FnMut(SurfaceId, InputEvent) -> Option<InputEvent> + 'static,
    ) -> Self {
        self.setup.input_filter = Some(Box::new(filter));
        self
    }

    /// Locks the session and shows a lock surface composed by `compose` on every output of the host. Only an unlock
    /// through `handle` ends the lock.
    ///
    /// # Errors
    ///
    /// Fails when the app has a session lock already.
    pub fn session_lock(
        &mut self,
        handle: SessionLock,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Result<(), SetupError> {
        if self.setup.session_lock.is_some() {
            return Err(SetupError::SessionLockConfigured);
        }
        self.setup.session_lock = Some(LockConfiguration::new(
            handle,
            self.platform.clone(),
            compose,
        ));
        Ok(())
    }

    /// Returns the handle through which the app runs actions on its views. Each action selects its surface by the
    /// id assigned when the surface was created, which no other surface gets.
    #[must_use]
    pub fn view_actions(&self) -> ViewActions {
        self.setup.action_queue.view_actions()
    }

    /// Sets `callback`, which runs for each event of a drag offered by another client and returns whether the drag
    /// is accepted at its window and position. Returning `None` rejects the drag. On `DragEvent::Drop`, the callback
    /// must read what it needs about the destination, because the data of the drop arrives later.
    #[must_use]
    pub fn on_drag(
        mut self,
        callback: impl FnMut(DragEvent, &mut View) -> Option<DropAcceptance> + 'static,
    ) -> Self {
        self.setup.drags.on_drag(callback);
        self
    }

    /// Sets `callback`, which runs when the receipt of the data of a drop ends, with the data or the error in the
    /// event, and commits the drop. Returning true finishes the transfer, but only when the data was received.
    #[must_use]
    pub fn on_drop(mut self, callback: impl FnMut(DropEvent, &mut View) -> bool + 'static) -> Self {
        self.setup.drags.on_drop(callback);
        self
    }

    /// Sets `callback`, which runs for a drag gesture during an active pointer grab and can start a drag from the
    /// app. Returning `None` leaves the gesture to the built-in text drag when text transfers are enabled, and
    /// otherwise to the usual pointer handling.
    #[must_use]
    pub fn on_drag_source(
        mut self,
        callback: impl FnMut(DragGesture, &mut View) -> Option<DragRequest> + 'static,
    ) -> Self {
        self.setup.drags.on_drag_source(callback);
        self
    }

    /// Enables built-in drag and drop of text: dragging selected text out of a text field and dropping text into a
    /// text field. The drag and drop callbacks registered by the app run first. The built-in handling applies only to
    /// the drags that the app did not accept.
    #[must_use]
    pub fn text_transfers(mut self) -> Self {
        self.setup.drags.enable_text_transfers();
        self
    }

    /// Sets preferences of the app that override the preferences of the system, also after the system preferences
    /// change.
    #[must_use]
    pub fn preference_overrides(mut self, overrides: PreferenceOverrides) -> Self {
        self.setup.preference_overrides = overrides;
        self
    }

    /// Sets `callback`, which receives every physical key press and release before the view. Returning true consumes
    /// the key, so the view does not see it.
    #[must_use]
    pub fn on_keyboard(mut self, callback: impl FnMut(HostKey) -> bool + 'static) -> Self {
        self.setup.observers.key = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs each time a surface gains or loses the keyboard focus.
    #[must_use]
    pub fn on_keyboard_focus(mut self, callback: impl FnMut(KeyboardFocus) + 'static) -> Self {
        self.setup.observers.keyboard_focus = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs after a view is composed and can inspect the view without changing how the
    /// platform handles input. The callback receives the ID of the surface, the same ID used by view actions, and the
    /// viewport of the surface: its size in dp and its density.
    #[must_use]
    pub fn view_observer(
        mut self,
        callback: impl FnMut(SurfaceId, Viewport, &View) + 'static,
    ) -> Self {
        self.setup.observers.view = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn exit_when(mut self, predicate: impl FnMut() -> bool + 'static) -> Self {
        self.setup.run_end.exit_when(predicate);
        self
    }

    /// Turns incremental painting on or off. With `enabled` false, every frame repaints the whole surface, for
    /// comparisons or for a host that changes content in ways that the damage tracking does not detect.
    #[must_use]
    pub fn incremental(mut self, enabled: bool) -> Self {
        self.setup.renderer.full_redraw = !enabled;
        self
    }

    /// Draws the latest completed image of the scene source returned by `make` beneath the UI of each window. `make`
    /// runs on the thread of the renderer, which owns the source. The effect quality chosen for the UI does not change
    /// the deadlines of the source.
    #[must_use]
    pub fn scene<S: SceneSource + 'static>(
        mut self,
        make: impl FnOnce() -> S + Send + 'static,
    ) -> Self {
        self.setup.renderer.scene = Some(Box::new(move || -> Box<dyn SceneSource> {
            Box::new(make())
        }));
        self
    }

    /// Adds `setup`, which runs on the painter on the thread of the renderer before the first render, for example to
    /// install effects or to replace the chooser of the effect quality. Each setup runs after the setups added before
    /// it. The text rendering set by `setup` applies to measuring text for the views too.
    #[must_use]
    pub fn configure_painter(mut self, setup: impl FnOnce(&mut Painter) + Send + 'static) -> Self {
        let previous = self.setup.renderer.painter.take();
        self.setup.renderer.painter = Some(Box::new(move |painter| {
            if let Some(previous) = previous {
                previous(painter);
            }
            setup(painter);
        }));
        self
    }

    /// Sets `budget`, the time per frame available to the UI. The chooser of the effect quality cannot raise it.
    #[must_use]
    pub fn ui_budget(mut self, budget: Duration) -> Self {
        self.setup.ui_budget = Some(budget);
        self
    }

    /// Creates an app whose views get `platform`, the platform facts supplied by the app. A fact that the host
    /// reports replaces the supplied one.
    #[must_use]
    pub fn new(platform: PlatformFacts) -> Self {
        let watches = Watches::default();
        let renderer = RendererSetup {
            fonts: Fonts::default(),
            painter: None,
            scene: None,
            gpu_timing: false,
            full_redraw: false,
        };
        let setup = RuntimeSetup {
            surfaces: Vec::new(),
            panels_per_output: None,
            session_lock: None,
            renderer,
            ui_budget: None,
            run_end: RunEnd::default(),
            preference_overrides: PreferenceOverrides::default(),
            input_filter: None,
            observers: Observers::default(),
            clipboard: ClipboardTransfers::new(&watches),
            action_queue: ActionQueue::new(&watches),
            drags: Drags::new(&watches),
            watches,
        };
        Self { setup, platform }
    }

    #[must_use]
    pub fn fonts(mut self, fonts: Fonts) -> Self {
        self.setup.renderer.fonts = fonts;
        self
    }

    #[must_use]
    pub fn clipboard(&self) -> Clipboard {
        self.setup.clipboard.clipboard()
    }

    #[must_use]
    pub fn exit_after(mut self, duration: Duration) -> Self {
        self.setup.run_end.exit_after(duration);
        self
    }

    #[must_use]
    pub fn on_tick(mut self, tick: impl FnMut(Duration) + 'static) -> Self {
        self.setup.observers.tick = Some(Box::new(tick));
        self
    }

    /// Sets `callback`, which runs once for each render opportunity given by the platform, with the timing of the
    /// frame based on its presentation time.
    #[must_use]
    pub fn on_frame(mut self, callback: impl FnMut(FrameTime) + 'static) -> Self {
        self.setup.observers.frame = Some(Box::new(callback));
        self
    }

    /// Turns timing of the GPU work of every frame on or off. The measured time appears in `Submission::gpu` of the
    /// frame reports.
    #[must_use]
    pub fn gpu_timing(mut self, enabled: bool) -> Self {
        self.setup.renderer.gpu_timing = enabled;
        self
    }

    /// Sets `callback`, which receives a [`FrameReport`] when a frame is submitted, presented or discarded. The
    /// callback runs on the thread of the views, so it must return quickly and buffer writes to disk.
    #[must_use]
    pub fn frame_observer(mut self, callback: impl FnMut(FrameReport) + 'static) -> Self {
        self.setup.observers.frame_report = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn output_observer(
        mut self,
        observer: impl FnMut(SurfaceId, OutputInfo) + 'static,
    ) -> Self {
        self.setup.observers.output = Some(Box::new(observer));
        self
    }

    /// Sets `observer`, which receives each component that failed in a frame, with the ID of the surface that shows
    /// the frame. Without an observer, the runtime logs each failed component as an error.
    #[must_use]
    pub fn failure_observer(
        mut self,
        observer: impl FnMut(SurfaceId, ComponentFailure) + 'static,
    ) -> Self {
        self.setup.observers.failure = Some(Box::new(observer));
        self
    }

    /// Adds a window that shows `compose`.
    ///
    /// # Errors
    ///
    /// Fails when the size of `window` is not finite or not above zero, or when its minimum size is not finite or
    /// below zero.
    pub fn window(
        &mut self,
        window: Window,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Result<(), SetupError> {
        if let Some(size) = window.size
            && let Err(source) = size.to_positive()
        {
            return Err(SetupError::WindowSize { size, source });
        }
        if let Some(size) = window.min_size
            && let Err(source) = size.to_non_negative()
        {
            return Err(SetupError::MinimumWindowSize { size, source });
        }
        self.setup.surfaces.push(SurfaceSetup::Window(
            window,
            View::new(self.platform.clone(), compose),
        ));
        Ok(())
    }

    pub fn layer(&mut self, layer: LayerSurface, compose: impl FnMut(&mut Ui) + 'static) {
        self.setup.surfaces.push(SurfaceSetup::Panel(
            layer,
            View::new(self.platform.clone(), compose),
        ));
    }

    /// Registers `factory`, which runs once for each output that appears and returns the layer surfaces to create on
    /// that output. The `State<MonitorInfo>` passed to the factory updates when the geometry of the output changes.
    /// The surfaces close when the output is removed.
    ///
    /// # Errors
    ///
    /// Fails when the app has a per-output layer factory already.
    pub fn layers_per_output(
        &mut self,
        factory: impl FnMut(State<MonitorInfo>) -> Vec<(LayerSurface, View)> + 'static,
    ) -> Result<(), SetupError> {
        if self.setup.panels_per_output.is_some() {
            return Err(SetupError::OutputLayersConfigured);
        }
        self.setup.panels_per_output = Some(PanelsPerOutput::new(factory));
        Ok(())
    }

    /// Converts the app into the setup for a run.
    pub(crate) fn into_setup(self) -> RuntimeSetup {
        self.setup
    }
}

#[cfg(test)]
mod tests;
