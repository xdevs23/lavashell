// SPDX-License-Identifier: GPL-3.0-only
use super::App;
use crate::logical::{whole_size, window_size};
use crate::{RunError, SetupError, Wayland};
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::Viewport;
use lavashell_core::output::OutputInfo;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragEvent, DropAcceptance};
use lavashell_core::ui::Ui;
use lavashell_core::view::{InputEvent, View};
use lavashell_renderer::vulkan::SceneSource;
use lavashell_renderer::{Fonts, Painter, RendererSetup};
use lavashell_runtime::{
    ActionQueue, Clipboard, ClipboardTransfers, DragGesture, DragRequest, Drags, DropEvent,
    HostKey, KeyboardFocus, LayerSurface, LockConfiguration, Observers, RunEnd, RuntimeSetup,
    SessionLock, SurfaceSetup, ViewActions, Watches, Window,
};
use std::time::{Duration, Instant};

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
        self.input_filter = Some(Box::new(filter));
        self
    }

    /// Locks the session through ext-session-lock-v1 and shows a lock surface composed by `compose` on every output
    /// that the compositor advertises. Only an unlock through `handle` ends the lock.
    ///
    /// # Errors
    ///
    /// Fails when the app has a session lock already.
    pub fn session_lock(
        &mut self,
        handle: SessionLock,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Result<(), SetupError> {
        if self.session_lock.is_some() {
            return Err(SetupError::SessionLockConfigured);
        }
        self.session_lock = Some(LockConfiguration::new(
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
        self.action_queue.view_actions()
    }

    /// Sets `callback`, which runs for each event of a drag offered by another client and returns whether the drag
    /// is accepted at its window and position. Returning `None` rejects the drag. On `DragEvent::Drop`, the callback
    /// must read what it needs about the destination, because the data of the drop arrives later.
    #[must_use]
    pub fn on_drag(
        mut self,
        callback: impl FnMut(DragEvent, &mut View) -> Option<DropAcceptance> + 'static,
    ) -> Self {
        self.drags.on_drag(callback);
        self
    }

    /// Sets `callback`, which runs when the receipt of the data of a drop ends, with the data or the error in the
    /// event, and commits the drop. Returning true finishes the Wayland transfer, but only when the data was received.
    #[must_use]
    pub fn on_drop(mut self, callback: impl FnMut(DropEvent, &mut View) -> bool + 'static) -> Self {
        self.drags.on_drop(callback);
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
        self.drags.on_drag_source(callback);
        self
    }

    /// Enables built-in drag and drop of text: dragging selected text out of a text field and dropping text into a
    /// text field. The drag and drop callbacks registered by the app run first. The built-in handling applies only to
    /// the drags that the app did not accept.
    #[must_use]
    pub fn text_transfers(mut self) -> Self {
        self.drags.enable_text_transfers();
        self
    }

    /// Sets preferences of the app that override the preferences of the system, also after the system preferences
    /// change.
    #[must_use]
    pub fn preference_overrides(mut self, overrides: PreferenceOverrides) -> Self {
        self.preference_overrides = overrides;
        self
    }

    /// Sets `callback`, which receives every physical key press and release before the view. Returning true consumes
    /// the key, so the view does not see it.
    #[must_use]
    pub fn on_keyboard(mut self, callback: impl FnMut(HostKey) -> bool + 'static) -> Self {
        self.observers.key = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs each time a surface gains or loses the keyboard focus.
    #[must_use]
    pub fn on_keyboard_focus(mut self, callback: impl FnMut(KeyboardFocus) + 'static) -> Self {
        self.observers.keyboard_focus = Some(Box::new(callback));
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
        self.observers.view = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn exit_when(mut self, predicate: impl FnMut() -> bool + 'static) -> Self {
        self.run_end.exit_when(predicate);
        self
    }

    /// Turns incremental painting on or off. With `enabled` false, every frame repaints the whole surface, for
    /// comparisons or for a host that changes content in ways that the damage tracking does not detect.
    #[must_use]
    pub fn incremental(mut self, enabled: bool) -> Self {
        self.full_redraw = !enabled;
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
        self.scene = Some(Box::new(move || -> Box<dyn SceneSource> {
            Box::new(make())
        }));
        self
    }

    /// Adds `setup`, which runs on the painter on the thread of the renderer before the first render, for example to
    /// install effects or to replace the chooser of the effect quality. Each setup runs after the setups added before
    /// it. The text rendering set by `setup` applies to measuring text for the views too.
    #[must_use]
    pub fn configure_painter(mut self, setup: impl FnOnce(&mut Painter) + Send + 'static) -> Self {
        let previous = self.painter_setup.take();
        self.painter_setup = Some(Box::new(move |painter| {
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
        self.ui_budget = Some(budget);
        self
    }

    /// Creates an app whose views get `platform`, the platform facts supplied by the app. A fact that the host
    /// reports replaces the supplied one.
    #[must_use]
    pub fn new(platform: PlatformFacts) -> Self {
        let watches = Watches::default();
        Self {
            surfaces: Vec::new(),
            output_layers: None,
            fonts: Fonts::default(),
            session_lock: None,
            run_end: RunEnd::default(),
            painter_setup: None,
            scene: None,
            gpu_timing: false,
            full_redraw: false,
            ui_budget: None,
            preference_overrides: PreferenceOverrides::default(),
            input_filter: None,
            observers: Observers::default(),
            clipboard: ClipboardTransfers::new(&watches),
            action_queue: ActionQueue::new(&watches),
            drags: Drags::new(&watches),
            watches,
            platform,
        }
    }

    #[must_use]
    pub fn fonts(mut self, fonts: Fonts) -> Self {
        self.fonts = fonts;
        self
    }

    #[must_use]
    pub fn clipboard(&self) -> Clipboard {
        self.clipboard.clipboard()
    }

    #[must_use]
    pub fn exit_after(mut self, duration: Duration) -> Self {
        self.run_end.exit_after(duration);
        self
    }

    #[must_use]
    pub fn on_tick(mut self, tick: impl FnMut(Duration) + 'static) -> Self {
        self.observers.tick = Some(Box::new(tick));
        self
    }

    /// Sets `callback`, which runs once for each render opportunity given by the compositor, with the timing of the
    /// frame based on its presentation time.
    #[must_use]
    pub fn on_frame(mut self, callback: impl FnMut(FrameTime) + 'static) -> Self {
        self.observers.frame = Some(Box::new(callback));
        self
    }

    /// Turns timing of the GPU work of every frame on or off. The measured time appears in `Submission::gpu` of the
    /// frame reports.
    #[must_use]
    pub fn gpu_timing(mut self, enabled: bool) -> Self {
        self.gpu_timing = enabled;
        self
    }

    /// Sets `callback`, which receives a [`FrameReport`] when a frame is submitted, presented or discarded. The
    /// callback runs on the thread of the views, so it must return quickly and buffer writes to disk.
    #[must_use]
    pub fn frame_observer(mut self, callback: impl FnMut(FrameReport) + 'static) -> Self {
        self.observers.frame_report = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn output_observer(
        mut self,
        observer: impl FnMut(SurfaceId, OutputInfo) + 'static,
    ) -> Self {
        self.observers.output = Some(Box::new(observer));
        self
    }

    /// Adds a window that shows `compose`.
    ///
    /// # Errors
    ///
    /// Fails when the size or the minimum size of `window` does not fit the protocol.
    pub fn window(
        &mut self,
        window: Window,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Result<(), SetupError> {
        if let Some(size) = window.size.filter(|&size| window_size(size).is_none()) {
            return Err(SetupError::WindowSize(size));
        }
        if let Some(minimum) = window
            .min_size
            .filter(|&minimum| whole_size(minimum).is_none())
        {
            return Err(SetupError::MinimumWindowSize(minimum));
        }
        self.surfaces.push(SurfaceSetup::Window(
            window,
            View::new(self.platform.clone(), compose),
        ));
        Ok(())
    }

    pub fn layer(&mut self, layer: LayerSurface, compose: impl FnMut(&mut Ui) + 'static) {
        self.surfaces.push(SurfaceSetup::Panel(
            layer,
            View::new(self.platform.clone(), compose),
        ));
    }

    /// Calls `startup` to add the surfaces, connects to the compositor and runs until the last
    /// surface closes.
    ///
    /// # Errors
    ///
    /// Fails with the error of `startup`, when no surface was added, and when the connection, a
    /// protocol the surfaces need, the renderer, a frame or a window move fails.
    pub fn run(
        mut self,
        startup: impl FnOnce(&mut Self) -> Result<(), SetupError>,
    ) -> Result<(), RunError> {
        super::standard_error::install();
        startup(&mut self).map_err(RunError::Setup)?;
        if self.surfaces.is_empty() && self.session_lock.is_none() && self.output_layers.is_none() {
            return Err(RunError::Setup(SetupError::NoSurfaces));
        }
        lavashell_runtime::drive::<Wayland>(self.into_setup(Instant::now())).map_err(RunError::Run)
    }

    fn into_setup(self, start: Instant) -> RuntimeSetup {
        RuntimeSetup {
            surfaces: self.surfaces,
            panels_per_output: self.output_layers,
            session_lock: self.session_lock,
            renderer: RendererSetup {
                fonts: self.fonts,
                painter: self.painter_setup,
                scene: self.scene,
                gpu_timing: self.gpu_timing,
                full_redraw: self.full_redraw,
            },
            start,
            ui_budget: self.ui_budget,
            run_end: self.run_end,
            preference_overrides: self.preference_overrides,
            input_filter: self.input_filter,
            observers: self.observers,
            watches: self.watches,
            clipboard: self.clipboard,
            action_queue: self.action_queue,
            drags: self.drags,
        }
    }
}
