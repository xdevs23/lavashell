// SPDX-License-Identifier: GPL-3.0-only
use super::creation::SurfaceSpec;
use super::namespaces::SurfaceScene;
use super::{
    App, ClipboardState, CompositorState, Connection, Duration, EventQueue, Fonts, GlobalList,
    Instant, LoopHandle, OutputState, Painter, PainterSetup, Platform, QueueHandle, RegistryState,
    Request, Role, SeatDevices, SeatState, Shm, SurfaceProtocols, VulkanRenderer, WaylandSurface,
    Window, WindowDecorations, XdgShell, accessibility, focus_grab, output_layers, preferences,
    presentation, scene, session_lock, wl_surface,
};
use crate::Error;
use crate::logical::extent;
use lavashell_core::geometry::Size;
use lavashell_core::preferences::PreferenceSource;
use lavashell_core::view::Proposal;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;

fn window_extent(size: Size) -> Option<(u32, u32)> {
    extent(size.width, 0).zip(extent(size.height, 0))
}

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

/// Creates a painter on `gpu` and runs `setup` on it. The painter gets a GPU profiler when `gpu_timing` is set or
/// when its quality chooser picks the effect quality from measured GPU times.
fn painter(
    gpu: &mut VulkanRenderer,
    fonts: &Fonts,
    setup: Option<PainterSetup>,
    gpu_timing: bool,
) -> Result<Painter, Error> {
    let mut paint = gpu.painter(fonts).map_err(Error::CreatePainter)?;
    if let Some(setup) = setup {
        setup(&mut paint);
    }
    if gpu_timing || paint.uses_quality_chooser() {
        let profiler = gpu.create_profiler().map_err(Error::CreateProfiler)?;
        if gpu_timing {
            gpu.time_frames(profiler.clone());
        }
        paint.time_gpu(profiler);
    }
    Ok(paint)
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
            scene,
            gpu_timing,
            preference_overrides,
            config,
            ..
        } = app;
        let protocols = SurfaceProtocols::bind(globals, &qh);
        let mut gpu =
            VulkanRenderer::for_wayland(&connection.backend()).map_err(Error::CreateRenderer)?;
        let paint = painter(&mut gpu, &fonts, painter_setup, gpu_timing)?;
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
            .map(|configuration| {
                output_layers::Runtime::new(configuration, protocols.layers.as_ref())
            })
            .transpose()?;
        let start = Instant::now();
        let mut view_configuration = preferences::ViewConfiguration {
            preferences: PreferenceSource::default(),
            facts: preferences::HostFacts {
                locales: preferences::user_languages(),
                ..preferences::HostFacts::default()
            },
        };
        view_configuration
            .preferences
            .set_overrides(preference_overrides);
        let seats = SeatState::new(globals, &qh);
        let connection_source = WaylandSource::new(connection.clone(), queue)
            .insert(loop_handle.clone())
            .map_err(|failure| Error::WatchConnection(failure.error))?;
        let mut sources = vec![connection_source];
        let accessibility_events = accessibility::install(&loop_handle, &mut sources)?;
        preferences::install(&loop_handle, &mut sources)?;
        let mut platform = Self {
            focus_grabs: focus_grab::Capability::bind(globals, &qh),
            view_configuration,
            accessibility_events,
            output_layers,
            session_lock,
            sources,
            readable_sources: Vec::new(),
            surfaces: Vec::new(),
            gpu,
            paint,
            connection: connection.clone(),
            registry: RegistryState::new(globals),
            devices: seats.seats().map(SeatDevices::new).collect(),
            clipboard: ClipboardState::new(globals, &qh, seats.seats(), clipboard),
            seats,
            outputs,
            shm: Shm::bind(globals, &qh).map_err(Error::BindShm)?,
            compositor: CompositorState::bind(globals, &qh).map_err(Error::BindCompositor)?,
            protocols,
            loop_handle,
            config,
            presentation: presentation::Presentation::bind(globals, &qh),
            start,
            last_tick: Duration::ZERO,
            scene: scene.map(scene::SceneRuntime::new),
            qh,
        };
        for request in requests {
            platform.create_requested_surface(request)?;
        }
        Ok(platform)
    }

    fn create_requested_surface(&mut self, request: Request) -> Result<(), Error> {
        match request {
            Request::Window(options, start) => {
                let (mut view, scene) = SurfaceScene::start(start);
                let unbounded = Proposal {
                    width: None,
                    height: None,
                };
                let size = match options.size {
                    Some(size) => size,
                    None => (view.size_that_fits(unbounded, &mut self.paint))
                        .map_err(Error::MeasureView)?,
                };
                let whole = window_extent(size).filter(|&(width, height)| width > 0 && height > 0);
                let spec = SurfaceSpec {
                    view,
                    scene,
                    size: whole.ok_or(Error::WindowSize(size))?,
                    transparent: options.transparent || options.shape.is_some(),
                    output: options.output,
                    hdr_metadata: options.hdr_metadata,
                    background_blur: options.background_blur,
                    shape: options.shape.clone(),
                    drag_region: options.drag_region.clone(),
                    accessible_name: options.title.clone(),
                };
                self.create_surface(spec, |platform, wl, _| platform.window_role(wl, &options))
            }
            Request::Layer(options, start) => {
                let (mut view, scene) = SurfaceScene::start(start);
                let context = self.layer_context(&options, &mut view)?;
                let spec = SurfaceSpec::layer(&options, view, scene, context)?;
                self.create_surface(spec, |platform, wl, id| {
                    platform.layer_role(wl, None, &options, context, id)
                })
            }
            Request::Lock(output) => self.add_lock_output(&output),
        }
    }

    fn window_role(&self, wl: &wl_surface::WlSurface, options: &Window) -> Result<Role, Error> {
        let xdg = self.protocols.xdg.as_ref().ok_or(Error::NoXdgShell)?;
        let decorations = if options.shape.is_some() {
            WindowDecorations::ClientOnly
        } else {
            WindowDecorations::RequestServer
        };
        let window = xdg.create_window(wl.clone(), decorations, &self.qh);
        window.set_title(&options.title);
        window.set_app_id(&options.app_id);
        let min_size = (options.min_size)
            .map(|minimum| window_extent(minimum).ok_or(Error::MinimumWindowSize(minimum)))
            .transpose()?;
        window.set_min_size(min_size);
        if options.fullscreen {
            window.set_fullscreen(None);
        }
        window.commit();
        Ok(Role::Window { window })
    }
}
