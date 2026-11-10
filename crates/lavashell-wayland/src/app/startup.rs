// SPDX-License-Identifier: GPL-3.0-only
use super::control::{Control, RoleControl};
use super::creation::NativeSpec;
use super::{
    App, AppConfig, Platform, Request, Role, SeatDevices, SurfaceProtocols, accessibility,
    focus_grab, output_layers, preferences, presentation, scene_reports, scheduler_outputs,
};
use crate::Error;
use crate::clipboard::ClipboardState;
use crate::logical::{whole_size, window_size};
use crate::wayland_presentation::WaylandPresentation;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::view::View;
use lavashell_platform::{Role as SurfaceRole, SettingsEvent, SurfaceSpec};
use lavashell_renderer::{RendererSetup, RendererState};
use lavashell_runtime::{
    InputRouter, OutputRecords, PanelRecords, Redraw, ScenePipeline, SchedulerThread,
    SurfaceRecords, TextClipboard, Window,
};
use sctk::compositor::CompositorState;
use sctk::output::OutputState;
use sctk::reexports::calloop::LoopHandle;
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Connection, EventQueue, QueueHandle};
use sctk::registry::RegistryState;
use sctk::seat::SeatState;
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::{XdgShell, window::WindowDecorations};
use sctk::shm::Shm;
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

impl SurfaceProtocols {
    fn bind(globals: &GlobalList, qh: &QueueHandle<Platform>) -> Self {
        Self {
            xdg: XdgShell::bind(globals, qh).ok(),
            layers: globals.bind(qh, 1..=5, ()).ok(),
            viewporter: globals.bind(qh, 1..=1, ()).ok(),
            fractional: globals.bind(qh, 1..=1, ()).ok(),
            background: globals.bind(qh, 1..=1, ()).ok(),
        }
    }
}

/// Redraws every frame while the app ticks, observes each frame or shows a scene, and otherwise only after a change.
fn redraw_for(config: &AppConfig, renderer: &RendererState<Control>) -> Redraw {
    if config.tick.is_some() || config.on_frame.is_some() || renderer.shows_scene() {
        Redraw::Continuous
    } else {
        Redraw::OnChange
    }
}

impl Platform {
    pub(super) fn new(
        connection: &Connection,
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        queue: EventQueue<Self>,
        app: App,
        loop_handle: LoopHandle<'static, Self>,
    ) -> Result<Self, Error> {
        let App {
            requests,
            output_layers,
            fonts,
            session_lock,
            clipboard,
            painter_setup,
            scene,
            gpu_timing,
            preference_overrides,
            input_filter,
            config,
            run_end,
            ..
        } = app;
        let protocols = SurfaceProtocols::bind(globals, &qh);
        let start = Instant::now();
        let setup = RendererSetup {
            fonts,
            painter: painter_setup,
            scene,
            gpu_timing,
            full_redraw: config.full_redraw,
            start,
        };
        let wayland = WaylandPresentation::new(connection.backend());
        let renderer = RendererState::new(&wayland, setup).map_err(Error::Render)?;
        let outputs = OutputState::new(globals, &qh);
        let output_layers = output_layers
            .map(|panels| output_layers::Runtime::new(panels, protocols.layers.as_ref()))
            .transpose()?;
        let mut surface_records = SurfaceRecords::new(preference_overrides);
        surface_records.change_settings(SettingsEvent::Locales(preferences::user_languages()));
        let seats = SeatState::new(globals, &qh);
        let connection_source = WaylandSource::new(connection.clone(), queue)
            .insert(loop_handle.clone())
            .map_err(|failure| Error::WatchConnection(failure.error))?;
        let mut sources = vec![connection_source];
        let accessibility_events = accessibility::install(&loop_handle, &mut sources)?;
        preferences::install(&loop_handle, &mut sources)?;
        let builder_reports = scene_reports::install(&loop_handle, &mut sources)?;
        let scheduler_outputs = scheduler_outputs::install(&loop_handle, &mut sources)?;
        let redraw = redraw_for(&config, &renderer);
        let text = TextClipboard::new(clipboard.clipboard(), config.action_queue.view_actions());
        let mut platform = Self {
            focus_grabs: focus_grab::Grabs::bind(globals, &qh),
            accessibility_events,
            output_layers,
            session_lock: None,
            sources,
            watches: BTreeMap::new(),
            surface_records,
            panels: PanelRecords::default(),
            surface_requests: Vec::new(),
            scene: ScenePipeline::start(builder_reports)?,
            scene_stopped: false,
            scheduler: SchedulerThread::start(redraw, start, scheduler_outputs)?,
            scheduler_wake: None,
            scheduler_stopped: false,
            started_frames: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            renderer,
            connection: connection.clone(),
            registry: RegistryState::new(globals),
            devices: seats.seats().map(SeatDevices::new).collect(),
            input: InputRouter::new(input_filter),
            input_failure: None,
            clipboard: ClipboardState::new(globals, &qh, seats.seats(), clipboard, text),
            seats,
            outputs,
            output_records: OutputRecords::default(),
            shm: Shm::bind(globals, &qh).map_err(Error::BindShm)?,
            compositor: CompositorState::bind(globals, &qh).map_err(Error::BindCompositor)?,
            protocols,
            loop_handle,
            config,
            presentation: presentation::Presentation::bind(globals, &qh),
            run_end,
            start,
            last_tick: Duration::ZERO,
            qh,
        };
        for request in requests {
            let _requested_surface = platform.create_requested_surface(request)?;
        }
        if let Some(configuration) = session_lock {
            platform.start_lock(globals, configuration)?;
        }
        Ok(platform)
    }

    fn create_requested_surface(&mut self, request: Request) -> Result<SurfaceId, Error> {
        match request {
            Request::Window(window, mut view) => {
                let spec = window.spec(&mut view, self.renderer.shaper())?;
                let native = NativeSpec::window(window, view, &spec)?;
                self.create_surface(native, |platform, wl, _| {
                    platform.window_role(wl, &spec.role)
                })
            }
            Request::Layer(options, view) => self.create_layer(&options, view, None),
        }
    }

    fn window_role(
        &self,
        wl: &wl_surface::WlSurface,
        role: &SurfaceRole,
    ) -> Result<(Role, RoleControl), Error> {
        let SurfaceRole::Window {
            title,
            app_id,
            min_size,
            fullscreen,
            draws_decorations,
        } = role;
        let xdg = self.protocols.xdg.as_ref().ok_or(Error::NoXdgShell)?;
        let decorations = if *draws_decorations {
            WindowDecorations::ClientOnly
        } else {
            WindowDecorations::RequestServer
        };
        let window = xdg.create_window(wl.clone(), decorations, &self.qh);
        window.set_title(title);
        window.set_app_id(app_id);
        let min_size = (*min_size)
            .map(|minimum| whole_size(minimum).ok_or(Error::MinimumWindowSize(minimum)))
            .transpose()?;
        window.set_min_size(min_size);
        if *fullscreen {
            window.set_fullscreen(None);
        }
        window.commit();
        Ok((Role::Window { window }, RoleControl::Stateless))
    }
}

impl NativeSpec {
    /// The native surface of `window` that shows `view` and opens as `spec` describes.
    fn window(window: Window, view: View, spec: &SurfaceSpec) -> Result<Self, Error> {
        Ok(Self {
            view,
            size: window_size(spec.size).ok_or(Error::WindowSize(spec.size))?,
            appearance: spec.appearance,
            shape: window.shape,
            drag_region: window.drag_region,
            accessible_name: window.title,
            role_state: SurfaceState::default(),
        })
    }
}
