// SPDX-License-Identifier: GPL-3.0-only
use super::control::{Control, RoleControl};
use super::creation::NativeSpec;
use super::{
    App, AppConfig, Platform, Request, Role, SeatDevices, Stops, SurfaceProtocols, accessibility,
    focus_grab, output_layers, preferences, presentation, renderer_outputs, scene_reports,
    session_lock,
};
use crate::Error;
use crate::clipboard::ClipboardState;
use crate::logical::{whole_size, window_size};
use crate::wayland_presentation::WaylandPresentation;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::view::View;
use lavashell_platform::{Role as SurfaceRole, SettingsEvent, SurfaceSpec};
use lavashell_renderer::RendererSetup;
use lavashell_renderer::text::Shaper;
use lavashell_runtime::{
    FrameScheduler, InputRouter, OutputRecords, PanelRecords, Redraw, Renderer, RendererObserver,
    ScenePipeline, SurfaceRecords, Window,
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
            mut requests,
            output_layers,
            fonts,
            session_lock,
            clipboard,
            painter_setup,
            scene: scene_setup,
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
            scene: scene_setup,
            gpu_timing,
            full_redraw: config.full_redraw,
            start,
        };
        let redraw = redraw_mode(&config, &setup);
        let outputs = OutputState::new(globals, &qh);
        let session_lock = session_lock
            .map(|configuration| {
                session_lock::Runtime::new(
                    globals,
                    &qh,
                    configuration,
                    outputs.outputs(),
                    &mut requests,
                )
            })
            .transpose()?;
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
        let renderer_outputs = renderer_outputs::install(&loop_handle, &mut sources)?;
        let scene = ScenePipeline::start(builder_reports)?;
        let cpu_timing = config.observer.is_some();
        let (renderer, shaper) = start_renderer(connection, setup, cpu_timing, renderer_outputs)?;
        let mut platform = Self {
            focus_grabs: focus_grab::Capability::bind(globals, &qh),
            accessibility_events,
            output_layers,
            session_lock,
            sources,
            watches: BTreeMap::new(),
            surface_records,
            panels: PanelRecords::default(),
            surface_requests: Vec::new(),
            scene,
            renderer,
            stops: Stops::default(),
            shaper,
            scene_deadline: None,
            scheduler: FrameScheduler::new(redraw),
            started_frames: BTreeMap::new(),
            state_commits: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            retiring: BTreeMap::new(),
            connection: connection.clone(),
            registry: RegistryState::new(globals),
            devices: seats.seats().map(SeatDevices::new).collect(),
            input: InputRouter::new(input_filter),
            clipboard: ClipboardState::new(globals, &qh, seats.seats(), clipboard),
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
            if let Err(error) = platform.create_requested_surface(request) {
                return Err(platform.stopped_actor_failure(error));
            }
        }
        Ok(platform)
    }

    fn create_requested_surface(&mut self, request: Request) -> Result<SurfaceId, Error> {
        match request {
            Request::Window(window, mut view) => {
                let spec = window.spec(&mut view, &mut self.shaper)?;
                let native = NativeSpec::window(window, view, &spec)?;
                self.create_surface(native, |platform, wl, _| {
                    platform.window_role(wl, &spec.role)
                })
            }
            Request::Layer(options, view) => self.create_layer(&options, view, None),
            Request::Lock(output, view) => self
                .create_surface(NativeSpec::lock(view), |platform, wl, id| {
                    platform.lock_role(wl, &output, id)
                }),
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

/// Draws continuously while the app ticks, observes every frame or shows a scene, and otherwise only after a change.
fn redraw_mode(config: &AppConfig, setup: &RendererSetup) -> Redraw {
    if config.tick.is_some() || config.on_frame.is_some() || setup.shows_scene() {
        Redraw::Continuous
    } else {
        Redraw::OnChange
    }
}

/// Starts the renderer for the surfaces of `connection`, and the shaper that measures text with the text rendering of
/// that renderer.
fn start_renderer(
    connection: &Connection,
    setup: RendererSetup,
    cpu_timing: bool,
    outputs: RendererObserver,
) -> Result<(Renderer<Control>, Shaper), Error> {
    let fonts = setup.fonts.clone();
    let wayland = WaylandPresentation::new(connection.backend());
    let renderer =
        Renderer::start(wayland, setup, cpu_timing, outputs).map_err(Error::StartRenderer)?;
    let shaper =
        Shaper::with_rendering(&fonts, renderer.text_rendering()).map_err(Error::CreateShaper)?;
    Ok((renderer, shaper))
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
