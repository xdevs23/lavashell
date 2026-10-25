// SPDX-License-Identifier: GPL-3.0-only
use super::creation::SurfaceSpec;
use super::{
    Dp, FiniteDp, FrameOutcome, LayerLevel, LayerSurface, Platform, Proxy, QueueHandle, Role, Size,
    State, View, ZwlrLayerShellV1, ZwlrLayerSurfaceV1, wl_output, wl_surface, zwlr_layer_shell_v1,
    zwlr_layer_surface_v1,
};
use crate::Error;
use crate::logical::{extent, whole};
use crate::surface::{AxisPlacement, ExclusiveZone, Extent, Keyboard, LayerLayout};
use lavashell_core::view::Proposal;
use lavashell_renderer::SurfaceContents;
use zwlr_layer_surface_v1::Anchor;

#[derive(Clone, Copy)]
pub(super) struct Content {
    pub size: (u32, u32),
    pub input: bool,
}

pub(super) struct Runtime {
    pub composition_order: i32,
    pub focus: Option<super::focus_grab::Runtime>,
    pub proxy: ZwlrLayerSurfaceV1,
    layout: Option<State<LayerLayout>>,
    requested: LayerLayout,
    context: LayerContext,
    applied: ProtocolLayout,
    keyboard: Keyboard,
    queued_keyboard: bool,
    committed_keyboard: bool,
    content: Option<Content>,
    pending: bool,
}

impl Runtime {
    /// Queues the layer-shell requests for the current layout and keyboard setting, so that they are committed
    /// together with the buffer of the frame being composed.
    pub fn update(&mut self) {
        self.lay_out(self.current_layout(), self.context);
        let keyboard = self.keyboard.enabled();
        if keyboard != self.queued_keyboard {
            apply_keyboard(&self.proxy, keyboard);
            self.queued_keyboard = keyboard;
            self.pending = true;
        }
    }

    /// The size proposal within which the content of the layer is measured, or `None` when no length of
    /// the layer follows its content.
    pub fn content_proposal(&self) -> Option<Proposal> {
        let layout = self.current_layout();
        [layout.horizontal, layout.vertical]
            .into_iter()
            .any(|placement| placement.extent() == Some(Extent::Content))
            .then(|| content_proposal(layout))
    }

    /// Queues the geometry that follows from `content`, the size measured for the content of the layer
    /// after its view changed.
    pub fn content_measured(&mut self, content: Size) {
        self.lay_out(self.current_layout(), LayerContext { content });
    }

    fn current_layout(&self) -> LayerLayout {
        self.layout.as_ref().map_or(self.requested, State::get)
    }

    /// Queues the geometry of `requested` within `context` once either changed.
    fn lay_out(&mut self, requested: LayerLayout, context: LayerContext) {
        if (requested, context) == (self.requested, self.context) {
            return;
        }
        self.requested = requested;
        self.context = context;
        match ProtocolLayout::new(requested, context) {
            Ok(next) if next != self.applied => {
                apply(&self.proxy, Some(self.applied), next);
                self.applied = next;
                self.pending = true;
            }
            Ok(_) => {}
            Err(error) => log::warn!("the layer keeps its previous layout: {error}"),
        }
    }

    pub fn commit(&mut self, wl: &wl_surface::WlSurface, content_current: bool) -> bool {
        if !self.pending || !content_current {
            return false;
        }
        wl.commit();
        self.submitted();
        true
    }

    pub fn update_transaction(
        &mut self,
        wl: &wl_surface::WlSurface,
        view: &mut View,
        content_current: bool,
    ) {
        self.update();
        if self.pending && !self.commit(wl, content_current) {
            view.invalidate();
        }
    }

    pub fn presented(&mut self, frame: FrameOutcome, content: Content) {
        if frame == FrameOutcome::Shown {
            self.submitted();
            self.content = Some(content);
        }
    }

    pub fn input_enabled(&self) -> bool {
        self.content.is_some_and(|content| content.input)
    }

    pub fn content_current(&self, size: (u32, u32)) -> bool {
        self.content.is_some_and(|content| content.size == size)
    }

    fn submitted(&mut self) {
        self.committed_keyboard = self.queued_keyboard;
        self.pending = false;
    }

    pub fn keyboard_active(&self) -> bool {
        self.keyboard.enabled()
    }

    pub fn keyboard_enabled(&self) -> bool {
        !self.pending && self.committed_keyboard && self.keyboard_active()
    }

    fn new(
        proxy: ZwlrLayerSurfaceV1,
        options: &LayerSurface,
        context: LayerContext,
        applied: ProtocolLayout,
    ) -> Self {
        Self {
            composition_order: options.composition_order,
            focus: options
                .focus_grab
                .clone()
                .map(super::focus_grab::Runtime::new),
            proxy,
            layout: options.layout_state.clone(),
            requested: options.current_layout(),
            context,
            applied,
            keyboard: options.keyboard_configuration(),
            queued_keyboard: options.keyboard_enabled(),
            // The first commit only asks the compositor for a configure event and contains no buffer, so no keyboard
            // setting has been committed with content yet.
            committed_keyboard: false,
            content: None,
            pending: true,
        }
    }
}

impl SurfaceSpec {
    pub(super) fn layer(
        options: &LayerSurface,
        view: View,
        context: LayerContext,
    ) -> Result<Self, Error> {
        Ok(Self {
            view,
            size: initial_size(options, context)?,
            transparent: true,
            output: options.output,
            hdr_metadata: options.hdr_metadata,
            background_blur: options.background_blur,
            shape: options.shape.clone(),
            drag_region: None,
            accessible_name: options.namespace.clone(),
        })
    }
}

impl Platform {
    /// Measures the content of `view` within the fixed lengths of the layout in `options`. The result
    /// sets the lengths of the layer that follow its content.
    pub(super) fn layer_context(
        &mut self,
        options: &LayerSurface,
        view: &mut View,
    ) -> Result<LayerContext, Error> {
        let proposal = content_proposal(options.current_layout());
        let content = view.size_that_fits(proposal, &mut self.paint);
        Ok(LayerContext {
            content: content.map_err(Error::MeasureView)?,
        })
    }

    /// Gives `wl` the layer role on `output` and commits `wl`, so that the compositor sends the first configure event.
    pub(super) fn layer_role(
        &self,
        wl: &wl_surface::WlSurface,
        output: Option<&wl_output::WlOutput>,
        options: &LayerSurface,
        context: LayerContext,
        id: usize,
    ) -> Result<Role, Error> {
        let shell = self.protocols.layers.as_ref().ok_or(Error::NoLayerShell)?;
        let layout = ProtocolLayout::new(options.current_layout(), context)?;
        let proxy = create(shell, wl, output, options, layout, &self.qh, id);
        wl.commit();
        Ok(Role::Layer(Box::new(Runtime::new(
            proxy, options, context, layout,
        ))))
    }

    pub(super) fn update_layers(&mut self) -> Result<(), Error> {
        for surface in &mut self.surfaces {
            if !surface.lifecycle.is_open() {
                continue;
            }
            let content_current = match &surface.target {
                Some(target) if !surface.needs_visual_frame() => {
                    let contents = target.contents(surface.physical_size()?);
                    contents.map_err(Error::ReadSurfaceContents)? == SurfaceContents::Current
                }
                _ => false,
            };
            let Some(Role::Layer(layer)) = surface.lifecycle.role_mut() else {
                continue;
            };
            layer.update_transaction(&surface.wl, &mut surface.view, content_current);
        }
        Ok(())
    }
}

/// The size proposal for measuring the content of a layer with `layout`. An axis with a fixed length
/// proposes that length. An axis that fills or follows the content proposes no bound.
fn content_proposal(layout: LayerLayout) -> Proposal {
    let proposal = |placement: AxisPlacement| match placement.extent() {
        Some(Extent::Dp(length)) => Some(length.get()),
        Some(Extent::Content) | None => None,
    };
    Proposal {
        width: proposal(layout.horizontal),
        height: proposal(layout.vertical),
    }
}

fn create(
    shell: &ZwlrLayerShellV1,
    wl: &wl_surface::WlSurface,
    output: Option<&wl_output::WlOutput>,
    options: &LayerSurface,
    layout: ProtocolLayout,
    qh: &QueueHandle<Platform>,
    id: usize,
) -> ZwlrLayerSurfaceV1 {
    let layer = match options.level {
        LayerLevel::Background => zwlr_layer_shell_v1::Layer::Background,
        LayerLevel::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
        LayerLevel::Top => zwlr_layer_shell_v1::Layer::Top,
        LayerLevel::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
    };
    let surface = shell.get_layer_surface(wl, output, layer, options.namespace.clone(), qh, id);
    apply(&surface, None, layout);
    apply_keyboard(&surface, options.keyboard_enabled());
    surface
}

/// The edges of the output to which a layer is anchored along one axis, where `start` and `end` are the two edges of
/// that axis.
fn protocol_anchor(placement: AxisPlacement, start: Anchor, end: Anchor) -> Anchor {
    match placement {
        AxisPlacement::Start(_) => start,
        AxisPlacement::End(_) => end,
        AxisPlacement::Center(_) => Anchor::empty(),
        AxisPlacement::Fill => start | end,
    }
}

fn apply_keyboard(proxy: &ZwlrLayerSurfaceV1, enabled: bool) {
    proxy.set_keyboard_interactivity(if enabled {
        if proxy.version() >= 4 {
            zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand
        } else {
            zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive
        }
    } else {
        zwlr_layer_surface_v1::KeyboardInteractivity::None
    });
}

/// The measured values against which a layer layout resolves the lengths that depend on the layer itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LayerContext {
    /// The size of the content, measured when the layer is created and again after its view
    /// changes while a length follows the content.
    pub content: Size,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Axis {
    Horizontal,
    Vertical,
}

/// The axis that crosses the edge a layer is anchored to, or `None` when the layer is not anchored to a single edge,
/// because the compositor then ignores the exclusive zone of the layer.
fn zone_axis(layout: LayerLayout) -> Option<Axis> {
    let single_edge = |placement: AxisPlacement| match placement {
        AxisPlacement::Start(_) | AxisPlacement::End(_) => true,
        AxisPlacement::Center(_) | AxisPlacement::Fill => false,
    };
    match (single_edge(layout.horizontal), single_edge(layout.vertical)) {
        (false, true) => Some(Axis::Vertical),
        (true, false) => Some(Axis::Horizontal),
        _ => None,
    }
}

/// The size of a layer before the first configure event arrives. An axis that fills starts at
/// one logical pixel.
pub(super) fn initial_size(
    options: &LayerSurface,
    context: LayerContext,
) -> Result<(u32, u32), Error> {
    Ok((
        protocol_extent(options.horizontal, context.content.width)?.max(1),
        protocol_extent(options.vertical, context.content.height)?.max(1),
    ))
}

/// The length that the layer-shell protocol expects along one axis. The protocol expects zero for an
/// axis that fills the space between its anchors, so every other length is at least one logical pixel.
/// `content` is the measured length of the content along the axis.
fn protocol_extent(placement: AxisPlacement, content: Dp) -> Result<u32, Error> {
    match placement.extent() {
        None => Ok(0),
        Some(Extent::Content) => extent(content, 1).ok_or(Error::LayerContentLength(content)),
        Some(Extent::Dp(length)) => extent(length.get(), 1).ok_or(Error::LayerLength(length)),
    }
}

/// The exclusive zone that the layer-shell protocol expects. The protocol expects -1 for a layer that
/// extends under the zones reserved by other layers. `extent` is the protocol length of the layer
/// across its anchored edge, or `None` when the layer is not anchored to a single edge.
fn protocol_zone(zone: ExclusiveZone, extent: Option<u32>) -> Result<i32, Error> {
    match zone {
        ExclusiveZone::Reserve(length) => whole(length.get()).ok_or(Error::ExclusiveZone(length)),
        ExclusiveZone::LayerExtent => {
            i32::try_from(extent.unwrap_or(0)).map_err(Error::ReserveLayerExtent)
        }
        ExclusiveZone::IgnoreOthers => Ok(-1),
    }
}

/// The whole numbers sent to the compositor for a layout.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ProtocolLayout {
    anchor: Anchor,
    width: u32,
    height: u32,
    zone: i32,
    margins: ProtocolMargins,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProtocolMargins {
    top: i32,
    right: i32,
    bottom: i32,
    left: i32,
}

impl ProtocolLayout {
    fn new(layout: LayerLayout, context: LayerContext) -> Result<Self, Error> {
        let margin = |length: FiniteDp| whole(length.get()).ok_or(Error::LayerMargin(length));
        let margins = layout.margins;
        let width = protocol_extent(layout.horizontal, context.content.width)?;
        let height = protocol_extent(layout.vertical, context.content.height)?;
        let across = zone_axis(layout).map(|axis| match axis {
            Axis::Horizontal => width,
            Axis::Vertical => height,
        });
        Ok(Self {
            anchor: protocol_anchor(layout.horizontal, Anchor::Left, Anchor::Right)
                | protocol_anchor(layout.vertical, Anchor::Top, Anchor::Bottom),
            width,
            height,
            zone: protocol_zone(layout.exclusive_zone, across)?,
            margins: ProtocolMargins {
                top: margin(margins.top)?,
                right: margin(margins.right)?,
                bottom: margin(margins.bottom)?,
                left: margin(margins.left)?,
            },
        })
    }
}

fn apply(proxy: &ZwlrLayerSurfaceV1, previous: Option<ProtocolLayout>, next: ProtocolLayout) {
    for request in GeometryRequest::needed(previous, next) {
        request.send(proxy, next);
    }
}

/// A request of the layer surface protocol that sets one part of the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeometryRequest {
    Size,
    Anchor,
    ExclusiveZone,
    Margin,
}

impl GeometryRequest {
    /// Every geometry request, in the order the layer-shell protocol declares them.
    const ALL: [Self; 4] = [Self::Size, Self::Anchor, Self::ExclusiveZone, Self::Margin];

    /// The requests whose values differ between the layouts, or every request for a new protocol object.
    fn needed(previous: Option<ProtocolLayout>, next: ProtocolLayout) -> Vec<Self> {
        (Self::ALL.into_iter())
            .filter(|request| previous.is_none_or(|previous| request.differs(previous, next)))
            .collect()
    }

    fn differs(self, previous: ProtocolLayout, next: ProtocolLayout) -> bool {
        match self {
            Self::Size => (previous.width, previous.height) != (next.width, next.height),
            Self::Anchor => previous.anchor != next.anchor,
            Self::ExclusiveZone => previous.zone != next.zone,
            Self::Margin => previous.margins != next.margins,
        }
    }

    fn send(self, proxy: &ZwlrLayerSurfaceV1, layout: ProtocolLayout) {
        match self {
            Self::Size => proxy.set_size(layout.width, layout.height),
            Self::Anchor => proxy.set_anchor(layout.anchor),
            Self::ExclusiveZone => proxy.set_exclusive_zone(layout.zone),
            Self::Margin => {
                let ProtocolMargins {
                    top,
                    right,
                    bottom,
                    left,
                } = layout.margins;
                proxy.set_margin(top, right, bottom, left);
            }
        }
    }
}

#[cfg(test)]
mod focus_wire;
#[cfg(test)]
mod focus_wire_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;
