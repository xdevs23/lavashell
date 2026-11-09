// SPDX-License-Identifier: GPL-3.0-only
use super::layer::{LayerConfigure, LayerControl, LayerRequest};
use super::presentation::Feedback;
use crate::logical::whole;
use crate::translator::Translator;
use lavashell_core::geometry::Rect;
use lavashell_core::prelude::{Density, Viewport};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use lavashell_renderer::{FrameOutcome, PresentFrame, SurfaceControl};
use sctk::reexports::client::protocol::{wl_compositor, wl_region, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1;
use sctk::reexports::protocols::wp::presentation_time::client::wp_presentation::WpPresentation;
use sctk::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;
use smithay_client_toolkit as sctk;

mod error;

pub use error::ControlError;

/// The protocol objects of a native surface, used by the requests for its frames, and the connection that sends the
/// requests.
pub(super) struct SurfaceObjects {
    pub connection: Connection,
    pub wl: wl_surface::WlSurface,
    pub viewport: Option<WpViewport>,
    pub presentation: Option<WpPresentation>,
    pub compositor: wl_compositor::WlCompositor,
    pub background: Option<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    pub role_control: RoleControl,
}

/// The requests made by the role of a native surface for the state of the surface.
pub(super) enum RoleControl {
    /// The role of a window or a lock surface, which makes no requests for the state.
    Stateless,
    Layer(LayerControl),
}

/// The requests that the draw of each frame of one native surface applies.
pub struct Control {
    surface: SurfaceId,
    objects: SurfaceObjects,
    qh: QueueHandle<Translator>,
    /// Whether a frame callback was requested for a frame that was not shown. The next shown frame commits it.
    frame_requested: bool,
    /// The surface state applied by the host, or to be applied with the next commit.
    applied: SurfaceState,
    /// The requests for the state of the frame being drawn, made by `prepare` and sent by `before_present`.
    pending: Vec<StateRequest>,
}

/// A request for one part of the state of a native surface, in protocol units.
enum StateRequest {
    InputRegion(Option<Vec<ProtocolRect>>),
    BlurRegion(Option<Vec<ProtocolRect>>),
    Layer(LayerRequest),
}

/// A rectangle of whole logical pixels.
struct ProtocolRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Control {
    /// The control of the native surface `surface`, whose role sent `applied` to the host already.
    pub(super) fn new(
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

    /// The control of the layer role, or `None` when the surface has no layer role.
    fn layer_mut(&mut self) -> Option<&mut LayerControl> {
        match &mut self.objects.role_control {
            RoleControl::Layer(layer) => Some(layer),
            RoleControl::Stateless => None,
        }
    }

    /// Sends the requests for the parts of `state` that differ from the state applied last. The host applies them with
    /// the next commit.
    pub fn send_state(&mut self, state: &SurfaceState) -> Result<(), ControlError> {
        let requests = self.requests(state)?;
        self.send(requests, state);
        Ok(())
    }

    /// The requests for the parts of `state` that differ from the state applied last.
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

    /// The request for `change`, or `None` for a part of the state that the role of the surface does not set.
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

    /// Sends `requests`, made for `state`, so that the host applies `state` with the next commit.
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

    /// Runs `set` with a region made of `rects`, or with `None` for `None`, and destroys the region afterwards.
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

    /// Records a configure event of the layer role until the frame that applies it.
    fn receive(&mut self, configure: LayerConfigure) {
        if let Some(layer) = self.layer_mut() {
            layer.receive_configure(configure);
        }
    }

    /// Sets the size of the surface in logical pixels, and makes the requests for the state of `frame`, sent by
    /// `before_present`.
    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), ControlError> {
        let Viewport { size, density } = frame.viewport;
        match &self.objects.viewport {
            Some(viewport) => {
                let (width, height) =
                    crate::logical::protocol_size(size).ok_or(ControlError::Destination(size))?;
                viewport.set_destination(width, height);
            }
            None => self.objects.wl.set_buffer_scale(buffer_scale(density)?),
        }
        self.pending = self.requests(frame.state)?;
        Ok(())
    }

    /// Sends the requests for the state of `frame`, then requests the frame callback and the presentation feedback
    /// committed when `frame` is presented.
    fn before_present(&mut self, frame: &PresentFrame) {
        let requests = std::mem::take(&mut self.pending);
        self.send(requests, frame.state);
        let wl = &self.objects.wl;
        if !self.frame_requested {
            let _dispatched_on_done = wl.frame(&self.qh, self.surface);
            self.frame_requested = true;
        }
        if let Some(presentation) = &self.objects.presentation {
            let feedback = Feedback {
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

    /// Sends the requests for `state` and the commit of the surface with the buffer shown by the surface to the
    /// compositor.
    fn commit_state(&mut self, state: &SurfaceState) -> Result<(), ControlError> {
        self.send_state(state)?;
        self.objects.wl.commit();
        self.objects.connection.flush().map_err(ControlError::Flush)
    }
}

/// The whole logical pixels of `region`, a region in dp.
fn protocol_region(region: &[Rect]) -> Result<Vec<ProtocolRect>, ControlError> {
    (region.iter())
        .map(|&rect| {
            let fit = || {
                Some(ProtocolRect {
                    x: whole(rect.x)?,
                    y: whole(rect.y)?,
                    width: whole(rect.width)?,
                    height: whole(rect.height)?,
                })
            };
            fit().ok_or(ControlError::Region(rect))
        })
        .collect()
}

/// The whole number of buffer pixels along one logical pixel at `density`. A surface without a viewport sets it
/// with `wl_surface.set_buffer_scale`, which accepts only whole numbers.
fn buffer_scale(density: Density) -> Result<i32, ControlError> {
    let unit = crate::logical::size((1, 1));
    let (scale, _) = unit
        .buffer_size(density)
        .map_err(ControlError::BufferSize)?;
    i32::try_from(scale).map_err(ControlError::BufferScale)
}
