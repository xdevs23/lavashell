// SPDX-License-Identifier: GPL-3.0-only
use super::render::NextRender;
use super::scene::SceneRuntime;
use super::{
    App, AppConfig, Clipboard, Connection, Dp, Duration, EventLoop, Fonts, FrameReport, FrameTime,
    HostKey, InputEvent, KeyboardFocus, LayerSurface, OutputInfo, Painter, Platform, PlatformFacts,
    Request, RunEnd, Size, SurfaceId, Ui, View, Viewport, Window, registry_queue_init,
    session_lock,
};
use crate::Error;

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
        self.config.input_filter = Some(Box::new(filter));
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
        handle: crate::SessionLock,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Result<(), Error> {
        if self.session_lock.is_some() {
            return Err(Error::SessionLockConfigured);
        }
        self.session_lock = Some(session_lock::Configuration::new(
            handle,
            self.platform.clone(),
            compose,
        ));
        Ok(())
    }

    /// Returns the handle through which the app runs actions on its views. Each action selects its surface by the
    /// id assigned when the surface was created, which no other surface gets.
    #[must_use]
    pub fn view_actions(&self) -> super::ViewActions {
        self.config.view_actions.clone()
    }

    /// Sets `callback`, which runs for each event of a drag offered by another client and returns whether the drag
    /// is accepted at its window and position. Returning `None` rejects the drag. On `DragEvent::Drop`, the callback
    /// must read what it needs about the destination, because the data of the drop arrives later.
    #[must_use]
    pub fn on_drag(
        mut self,
        callback: impl FnMut(
            crate::clipboard::DragEvent,
            &mut View,
        ) -> Option<lavashell_core::transfer::DropAcceptance>
        + 'static,
    ) -> Self {
        self.config.drag_observer = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs when the receipt of the data of a drop ends, with the data or the error in the
    /// event, and commits the drop. Returning true finishes the Wayland transfer, but only when the data was received.
    #[must_use]
    pub fn on_drop(
        mut self,
        callback: impl FnMut(crate::clipboard::DropEvent, &mut View) -> bool + 'static,
    ) -> Self {
        self.config.drop_observer = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs for a drag gesture during an active pointer grab and can start a drag from the
    /// app. Returning `None` leaves the gesture to the built-in text drag when text transfers are enabled, and
    /// otherwise to the usual pointer handling.
    #[must_use]
    pub fn on_drag_source(
        mut self,
        callback: impl FnMut(
            crate::clipboard::DragGesture,
            &mut View,
        ) -> Option<crate::clipboard::DragRequest>
        + 'static,
    ) -> Self {
        self.config.drag_source_observer = Some(Box::new(callback));
        self
    }

    /// Sets preferences of the app that override the preferences of the system, also after the system preferences
    /// change.
    #[must_use]
    pub fn preference_overrides(
        mut self,
        overrides: lavashell_core::preferences::PreferenceOverrides,
    ) -> Self {
        self.preference_overrides = overrides;
        self
    }

    /// Sets `callback`, which receives every physical key press and release before the view. Returning true consumes
    /// the key, so the view does not see it.
    #[must_use]
    pub fn on_keyboard(mut self, callback: impl FnMut(HostKey) -> bool + 'static) -> Self {
        self.config.key_observer = Some(Box::new(callback));
        self
    }

    /// Sets `callback`, which runs each time a surface gains or loses the keyboard focus.
    #[must_use]
    pub fn on_keyboard_focus(mut self, callback: impl FnMut(KeyboardFocus) + 'static) -> Self {
        self.config.focus_observer = Some(Box::new(callback));
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
        self.config.view_observer = Some(Box::new(callback));
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
        self.config.full_redraw = !enabled;
        self
    }

    /// Draws the latest completed image of `source` beneath the UI of each window. The effect quality chosen for the
    /// UI does not change the deadlines of `source`.
    #[must_use]
    pub fn scene(mut self, source: impl lavashell_renderer::vulkan::SceneSource + 'static) -> Self {
        self.scene = Some(Box::new(source));
        self
    }

    /// Adds `setup`, which runs on the painter before the first render, for example to install effects or to replace
    /// the chooser of the effect quality. Each setup runs after the setups added before it.
    #[must_use]
    pub fn configure_painter(mut self, setup: impl FnOnce(&mut Painter) + 'static) -> Self {
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
        self.config.ui_budget = Some(budget);
        self
    }

    /// Creates an app whose views get `platform`, the platform facts supplied by the app. A fact that the host
    /// reports replaces the supplied one.
    #[must_use]
    pub fn new(platform: PlatformFacts) -> Self {
        Self {
            requests: Vec::new(),
            output_layers: None,
            fonts: Fonts::default(),
            session_lock: None,
            run_end: RunEnd::default(),
            clipboard: Clipboard::default(),
            painter_setup: None,
            scene: None,
            gpu_timing: false,
            preference_overrides: lavashell_core::preferences::PreferenceOverrides::default(),
            config: AppConfig::default(),
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
        self.clipboard.clone()
    }

    #[must_use]
    pub fn exit_after(mut self, duration: Duration) -> Self {
        self.run_end.exit_after(duration);
        self
    }

    #[must_use]
    pub fn on_tick(mut self, tick: impl FnMut(Duration) + 'static) -> Self {
        self.config.tick = Some(Box::new(tick));
        self
    }

    /// Sets `callback`, which runs once for each render opportunity given by the compositor, with the timing of the
    /// frame based on its presentation time.
    #[must_use]
    pub fn on_frame(mut self, callback: impl FnMut(FrameTime) + 'static) -> Self {
        self.config.on_frame = Some(Box::new(callback));
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
    /// callback runs on the event loop, so it must return quickly and buffer its writes to disk.
    #[must_use]
    pub fn frame_observer(mut self, callback: impl FnMut(FrameReport) + 'static) -> Self {
        self.config.observer = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn output_observer(
        mut self,
        observer: impl FnMut(SurfaceId, OutputInfo) + 'static,
    ) -> Self {
        self.config.output_observer = Some(Box::new(observer));
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
    ) -> Result<(), Error> {
        let positive =
            |length: Dp| crate::logical::extent(length, 0).is_some_and(|whole| whole > 0);
        let valid = |size: Size| {
            crate::logical::extent(size.width, 0)
                .and(crate::logical::extent(size.height, 0))
                .is_some()
        };
        if let Some(size) = window
            .size
            .filter(|size| !positive(size.width) || !positive(size.height))
        {
            return Err(Error::WindowSize(size));
        }
        if let Some(minimum) = window.min_size.filter(|minimum| !valid(*minimum)) {
            return Err(Error::MinimumWindowSize(minimum));
        }
        self.requests.push(Request::Window(
            window,
            View::new(self.platform.clone(), compose),
        ));
        Ok(())
    }

    pub fn layer(&mut self, layer: LayerSurface, compose: impl FnMut(&mut Ui) + 'static) {
        self.requests.push(Request::Layer(
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
    /// protocol the surfaces need, the renderer or a frame fails.
    pub fn run(
        mut self,
        startup: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        super::standard_error::install();
        startup(&mut self)?;
        if self.requests.is_empty() && self.session_lock.is_none() && self.output_layers.is_none() {
            return Err(Error::NoSurfaces);
        }
        let connection = Connection::connect_to_env().map_err(Error::Connect)?;
        let (globals, queue) =
            registry_queue_init::<Platform>(&connection).map_err(Error::ReadGlobals)?;
        let qh = queue.handle();
        let mut event_loop: EventLoop<Platform> =
            EventLoop::try_new().map_err(Error::CreateEventLoop)?;
        let mut platform =
            Platform::new(&connection, &globals, qh, queue, self, event_loop.handle())?;
        loop {
            if let Some(layers) = &mut platform.output_layers {
                layers.check()?;
            }
            platform.update_layers()?;
            if platform.update_lock()? == session_lock::LockState::Released {
                break;
            }
            let next = platform.render()?;
            platform.update_focus_grabs();
            if platform.finished() {
                break;
            }
            platform.watch_readable_sources()?;
            let timeout = match next {
                NextRender::Immediately => Some(Duration::ZERO),
                NextRender::AfterEvent => platform.timeout(),
            };
            event_loop
                .dispatch(timeout, &mut platform)
                .map_err(Error::DispatchEvents)?;
        }
        Ok(())
    }
}

impl Platform {
    /// The time for which the event loop waits: until the earliest of the exit time, the next scene image
    /// and the next change of a surface. A scene deadline that has passed is left out, because the render
    /// before this wait handled it. The loop then waits for the frame callback of the surface that was drawn.
    fn timeout(&self) -> Option<Duration> {
        let elapsed = self.start.elapsed();
        let exit = self.run_end.exit_in(elapsed);
        let scene = (self.scene.as_ref())
            .and_then(SceneRuntime::wake)
            .filter(|deadline| *deadline > elapsed);
        let surfaces = (self.surfaces.iter()).filter_map(|(&id, surface)| {
            surface.wakes(self.surface_records.open_view(id)?, elapsed)
        });
        let wakes = scene
            .into_iter()
            .chain(surfaces)
            .map(|deadline| deadline.saturating_sub(elapsed));
        exit.into_iter().chain(wakes).min()
    }

    /// Whether the run ends now. The session lock and the per-output layers count as open until they end.
    fn finished(&mut self) -> bool {
        let anything_open = self.session_lock.is_some()
            || self.output_layers.is_some()
            || self.surface_records.any_open();
        (self.run_end).finished(self.start.elapsed(), anything_open)
    }
}
