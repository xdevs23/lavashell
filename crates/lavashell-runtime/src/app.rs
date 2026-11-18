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
use lavashell_renderer::{DamageMode, Fonts, FrameTiming, Painter};
use lavashell_vulkan_renderer::RendererSetup;
use lavashell_vulkan_renderer::vulkan::SceneSource;
use std::time::Duration;

mod error;

pub use error::SetupError;

pub struct App {
    setup: RuntimeSetup,
    platform: PlatformFacts,
}

impl App {
    #[must_use]
    pub fn input_filter(
        mut self,
        filter: impl FnMut(SurfaceId, InputEvent) -> Option<InputEvent> + 'static,
    ) -> Self {
        self.setup.input_filter = Some(Box::new(filter));
        self
    }

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

    #[must_use]
    pub fn view_actions(&self) -> ViewActions {
        self.setup.action_queue.view_actions()
    }

    #[must_use]
    pub fn on_drag(
        mut self,
        callback: impl FnMut(DragEvent, &mut View) -> Option<DropAcceptance> + 'static,
    ) -> Self {
        self.setup.drags.on_drag(callback);
        self
    }

    #[must_use]
    pub fn on_drop(mut self, callback: impl FnMut(DropEvent, &mut View) -> bool + 'static) -> Self {
        self.setup.drags.on_drop(callback);
        self
    }

    #[must_use]
    pub fn on_drag_source(
        mut self,
        callback: impl FnMut(DragGesture, &mut View) -> Option<DragRequest> + 'static,
    ) -> Self {
        self.setup.drags.on_drag_source(callback);
        self
    }

    #[must_use]
    pub fn text_transfers(mut self) -> Self {
        self.setup.drags.enable_text_transfers();
        self
    }

    #[must_use]
    pub fn preference_overrides(mut self, overrides: PreferenceOverrides) -> Self {
        self.setup.preference_overrides = overrides;
        self
    }

    #[must_use]
    pub fn on_keyboard(mut self, callback: impl FnMut(HostKey) -> bool + 'static) -> Self {
        self.setup.observers.key = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn on_keyboard_focus(mut self, callback: impl FnMut(KeyboardFocus) + 'static) -> Self {
        self.setup.observers.keyboard_focus = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn view_observer(
        mut self,
        callback: impl FnMut(SurfaceId, FrameTime, Viewport, &View) + 'static,
    ) -> Self {
        self.setup.observers.view = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn exit_when(mut self, predicate: impl FnMut() -> bool + 'static) -> Self {
        self.setup.run_end.exit_when(predicate);
        self
    }

    #[must_use]
    pub fn damage(mut self, mode: DamageMode) -> Self {
        self.setup.renderer.damage = mode;
        self
    }

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

    #[must_use]
    pub fn new(platform: PlatformFacts) -> Self {
        let watches = Watches::default();
        let renderer = RendererSetup {
            fonts: Fonts::default(),
            painter: None,
            scene: None,
            frame_timing: FrameTiming::Off,
            damage: DamageMode::Tracked,
        };
        let setup = RuntimeSetup {
            surfaces: Vec::new(),
            panels_per_output: None,
            session_lock: None,
            renderer,
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

    #[must_use]
    pub fn on_frame(mut self, callback: impl FnMut(FrameTime) + 'static) -> Self {
        self.setup.observers.frame = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn frame_timing(mut self, timing: FrameTiming) -> Self {
        self.setup.renderer.frame_timing = timing;
        self
    }

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

    #[must_use]
    pub fn failure_observer(
        mut self,
        observer: impl FnMut(SurfaceId, ComponentFailure) + 'static,
    ) -> Self {
        self.setup.observers.failure = Some(Box::new(observer));
        self
    }

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

    pub(crate) fn into_setup(self) -> RuntimeSetup {
        self.setup
    }
}

#[cfg(test)]
mod tests;
