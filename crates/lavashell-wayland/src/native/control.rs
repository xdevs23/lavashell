// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceExtensions;
use super::layer::{self, LayerConfigure, LayerControl, LayerRequest};
use super::presentation::PresentationFeedbackTarget;
use crate::logical::round_to_i32;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::geometry::{PixelScale, Rect};
use lavashell_core::prelude::{Density, Viewport};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use lavashell_platform::{FrameOutcome, PresentFrame, Role, SurfaceControl};
use sctk::reexports::client::protocol::{wl_compositor, wl_region, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1;
use sctk::reexports::protocols::wp::presentation_time::client::wp_presentation::WpPresentation;
use sctk::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;

mod error;

pub use error::ControlError;

pub(crate) struct SurfaceObjects {
    pub(super) connection: Connection,
    pub(super) wl: wl_surface::WlSurface,
    pub(super) viewport: Option<WpViewport>,
    pub(super) presentation: Option<WpPresentation>,
    pub(super) compositor: wl_compositor::WlCompositor,
    pub(super) background: Option<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    pub(super) role_control: RoleControl,
}

pub(crate) enum RoleControl {
    Stateless,
    Layer(LayerControl),
}

impl SurfaceObjects {
    pub(crate) fn new(
        connection: Connection,
        wl: wl_surface::WlSurface,
        extensions: &SurfaceExtensions,
        presentation: Option<WpPresentation>,
        compositor: wl_compositor::WlCompositor,
        role_control: RoleControl,
    ) -> Self {
        Self {
            connection,
            wl,
            viewport: extensions.viewport.clone(),
            presentation,
            compositor,
            background: extensions.background.clone(),
            role_control,
        }
    }
}

pub(crate) fn initial_role_state(role: &Role) -> SurfaceState {
    match role {
        Role::Panel(panel) => layer::role_state(panel.geometry),
        Role::Window(_) | Role::Lock { .. } => SurfaceState::default(),
    }
}

pub struct Control {
    surface: SurfaceId,
    objects: SurfaceObjects,
    qh: QueueHandle<Translator>,
    frame_requested: bool,
    applied: SurfaceState,
    pending: Vec<StateRequest>,
}

enum StateRequest {
    InputRegion(Option<Vec<ProtocolRect>>),
    BlurRegion(Option<Vec<ProtocolRect>>),
    Layer(LayerRequest),
}

struct ProtocolRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Control {
    pub(crate) fn new(
        surface: SurfaceId,
        objects: SurfaceObjects,
        applied: SurfaceState,
        qh: QueueHandle<Translator>,
    ) -> Self {
        Self {
            surface,
            objects,
            qh,
            frame_requested: false,
            applied,
            pending: Vec::new(),
        }
    }

    fn layer_mut(&mut self) -> Option<&mut LayerControl> {
        match &mut self.objects.role_control {
            RoleControl::Layer(layer) => Some(layer),
            RoleControl::Stateless => None,
        }
    }

    pub fn send_state(&mut self, state: &SurfaceState) -> Result<(), ControlError> {
        let requests = self.requests(state)?;
        self.send(requests, state);
        Ok(())
    }

    fn requests(&mut self, state: &SurfaceState) -> Result<Vec<StateRequest>, ControlError> {
        let mut requests = Vec::new();
        for change in state.changes() {
            if !self.applied.contains(&change)
                && let Some(request) = self.request(change)?
            {
                requests.push(request);
            }
        }
        Ok(requests)
    }

    fn request(&mut self, change: SurfaceChange) -> Result<Option<StateRequest>, ControlError> {
        let region = |rects: Option<Vec<Rect>>| rects.as_deref().map(protocol_region).transpose();
        Ok(match change {
            SurfaceChange::InputRegion(rects) => Some(StateRequest::InputRegion(region(rects)?)),
            SurfaceChange::BlurRegion(rects) => Some(StateRequest::BlurRegion(region(rects)?)),
            role_change => match &mut self.objects.role_control {
                RoleControl::Layer(layer) => layer.request(&role_change)?.map(StateRequest::Layer),
                RoleControl::Stateless => None,
            },
        })
    }

    fn send(&mut self, requests: Vec<StateRequest>, state: &SurfaceState) {
        if requests.is_empty() {
            return;
        }
        for request in requests {
            match request {
                StateRequest::InputRegion(region) => {
                    self.with_region(region, |region| self.objects.wl.set_input_region(region));
                }
                StateRequest::BlurRegion(region) => {
                    if let Some(background) = &self.objects.background {
                        self.with_region(region, |region| background.set_blur_region(region));
                    }
                }
                StateRequest::Layer(request) => {
                    if let Some(layer) = self.layer_mut() {
                        layer.send(request);
                    }
                }
            }
        }
        self.applied.clone_from(state);
    }

    fn with_region(
        &self,
        rects: Option<Vec<ProtocolRect>>,
        set: impl FnOnce(Option<&wl_region::WlRegion>),
    ) {
        let region = rects.map(|rects| {
            let region = self.objects.compositor.create_region(&self.qh, ());
            for rect in rects {
                region.add(rect.x, rect.y, rect.width, rect.height);
            }
            region
        });
        set(region.as_ref());
        if let Some(region) = region {
            region.destroy();
        }
    }
}

impl SurfaceControl for Control {
    type Error = ControlError;
    type Message = LayerConfigure;

    fn receive(&mut self, configure: LayerConfigure) {
        if let Some(layer) = self.layer_mut() {
            layer.receive_configure(configure);
        }
    }

    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), ControlError> {
        let Viewport { size, density } = frame.scaled_viewport.viewport;
        match &self.objects.viewport {
            Some(viewport) => {
                let (width, height) = crate::logical::to_protocol_surface_size(size)
                    .ok_or(ControlError::Destination(size))?;
                viewport.set_destination(width, height);
            }
            None => self.objects.wl.set_buffer_scale(buffer_scale(density)?),
        }
        self.pending = self.requests(frame.state)?;
        Ok(())
    }

    fn before_present(&mut self, frame: &PresentFrame) {
        let requests = std::mem::take(&mut self.pending);
        self.send(requests, frame.state);
        let wl = &self.objects.wl;
        if !self.frame_requested {
            let _dispatched_on_done = wl.frame(&self.qh, self.surface);
            self.frame_requested = true;
        }
        if let Some(presentation) = &self.objects.presentation {
            let feedback = PresentationFeedbackTarget {
                surface: self.surface,
                timing: frame.time,
            };
            let _dispatched_on_presented = presentation.feedback(wl, &self.qh, feedback);
        }
    }

    fn after_present(&mut self, outcome: FrameOutcome) {
        if outcome == FrameOutcome::Shown {
            self.frame_requested = false;
        }
    }

    fn commit_state(&mut self, state: &SurfaceState) -> Result<(), ControlError> {
        self.send_state(state)?;
        self.objects.wl.commit();
        self.objects.connection.flush().map_err(ControlError::Flush)
    }
}

fn protocol_region(region: &[Rect]) -> Result<Vec<ProtocolRect>, ControlError> {
    (region.iter())
        .map(|&rect| {
            let fit = || {
                Some(ProtocolRect {
                    x: round_to_i32(rect.x)?,
                    y: round_to_i32(rect.y)?,
                    width: round_to_i32(rect.width)?,
                    height: round_to_i32(rect.height)?,
                })
            };
            fit().ok_or(ControlError::Region(rect))
        })
        .collect()
}

fn buffer_scale(density: Density) -> Result<i32, ControlError> {
    let unit = Viewport::new(crate::logical::convert_to_size((1, 1)), density);
    let scale = (PixelScale::SQUARE.buffer_size(unit))
        .map_err(ControlError::BufferSize)?
        .width;
    i32::try_from(scale).map_err(ControlError::BufferScale)
}
