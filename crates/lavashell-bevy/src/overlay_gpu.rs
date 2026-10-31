// SPDX-License-Identifier: GPL-3.0-only
//! Draws the Lavashell overlay into the image of each `OverlayCamera` with the embedded renderer, on the Vulkan device
//! of Bevy. The overlay draws between two submissions of Bevy on the same queue.

use crate::texture_types::{vulkan_format, vulkan_usage};
use crate::timings::{OverlayImageTrace, OverlayTiming, TimingRecords};
use crate::vulkan_host::{image_of, shared_device, signal_on_queue};
use crate::{
    Error, ExtractedOverlay, OverlayCamera, OverlayFrame, extract_overlay, prepare_overlay_targets,
};
use ash::vk;
use bevy::{
    core_pipeline::core_3d::graph::{Core3d, Node3d},
    prelude::*,
    render::{
        ExtractSchedule, RenderApp,
        extract_component::ExtractComponentPlugin,
        pipelined_rendering::PipelinedRenderingPlugin,
        render_graph::{
            NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
        },
        renderer::{RenderContext, RenderQueue},
        view::ViewTarget,
    },
};
use lavashell_core::error::message_chain;
use lavashell_renderer::{BorrowedImage, EmbeddedVulkanRenderer, Fonts, Painter, ReadySignal};
use std::{cell::RefCell, mem::Discriminant, sync::mpsc::Sender, time::Instant};

pub struct LavashellOverlayPlugin {
    fonts: Fonts,
    records: Sender<OverlayTiming>,
}

impl LavashellOverlayPlugin {
    /// The overlay sends the timing record of every frame that it submits to `records`.
    ///
    /// # Safety
    /// Disable `PipelinedRenderingPlugin` and never submit to the `RenderQueue` of Bevy from another
    /// thread. The render schedule must order every submission to that queue before or after the
    /// overlay node, for the lifetime of the app, including shutdown.
    #[must_use]
    pub unsafe fn new(fonts: Fonts, records: Sender<OverlayTiming>) -> Self {
        Self { fonts, records }
    }
}

/// Installs the overlay renderer into the render world, unless Bevy renders pipelined.
struct InstallOverlay {
    fonts: Fonts,
    records: Sender<OverlayTiming>,
    pipelined: bool,
}

impl Command<Result<(), Error>> for InstallOverlay {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        if self.pipelined {
            return Err(Error::PipelinedRendering);
        }
        // SAFETY: No type of wgpu proves that the submissions to a queue are serialized, so `shared_device` needs that
        // promise from the caller and has no safe form. The check above confirms that pipelined rendering is off. The
        // contract of `LavashellOverlayPlugin::new` forbids submissions from other threads and orders the submissions
        // of Bevy with the overlay node.
        let device = unsafe { shared_device(world) }?;
        // SAFETY: `EmbeddedVulkanRenderer::new` needs the promise that the queue of the device is serialized, because
        // no type of wgpu proves it. The embedded API of lavashell-renderer has no safe constructor for a device
        // created by a host. The queue supports graphics, because wgpu draws on it. The contract of
        // `LavashellOverlayPlugin::new` serializes the submissions of Bevy with every call of the renderer. The
        // renderer is called only in the overlay node and when it is dropped.
        let gpu = unsafe { EmbeddedVulkanRenderer::new(device) }.map_err(Error::OverlayRenderer)?;
        let painter = gpu.painter(&self.fonts).map_err(Error::OverlayPainter)?;
        world.insert_non_send_resource(RefCell::new(GpuOverlay {
            painter,
            gpu,
            records: TimingRecords::new(self.records),
            sequence: 0,
            failure: None,
        }));
        Ok(())
    }
}

impl Plugin for LavashellOverlayPlugin {
    fn build(&self, app: &mut App) {
        let _chained_app = app
            .init_resource::<OverlayFrame>()
            .add_plugins(ExtractComponentPlugin::<OverlayCamera>::default())
            .add_systems(PostUpdate, prepare_overlay_targets);
        let _chained_render_app = app
            .sub_app_mut(RenderApp)
            .add_systems(ExtractSchedule, extract_overlay)
            .add_render_graph_node::<ViewNodeRunner<OverlayNode>>(Core3d, OverlayLabel)
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::EndMainPassPostProcessing,
                    OverlayLabel,
                    Node3d::Upscaling,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        // A plugin cannot return an error from `finish`, so the install runs as a command. Bevy passes an error of the
        // command to the error handler of the app.
        let install = InstallOverlay {
            fonts: self.fonts.clone(),
            records: self.records.clone(),
            pipelined: app.is_plugin_added::<PipelinedRenderingPlugin>(),
        };
        let world = app.sub_app_mut(RenderApp).world_mut();
        world.commands().queue(install);
        world.flush();
    }
}

struct GpuOverlay {
    painter: Painter,
    gpu: EmbeddedVulkanRenderer,
    records: TimingRecords,
    sequence: u64,
    /// The kind of error of the previous frame, so that a recurring error is logged once.
    failure: Option<Discriminant<Error>>,
}

impl GpuOverlay {
    fn failed(&mut self, error: &Error) {
        let kind = std::mem::discriminant(error);
        if self.failure != Some(kind) {
            log::error!(
                "the Lavashell overlay was not drawn: {}",
                message_chain(error)
            );
            self.failure = Some(kind);
        }
    }

    fn submitted(&mut self, timing: OverlayTiming) {
        self.failure = None;
        self.records.send(timing);
        self.sequence += 1;
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct OverlayLabel;

#[derive(Default)]
struct OverlayNode;

impl ViewNode for OverlayNode {
    type ViewQuery = (&'static ViewTarget, &'static OverlayCamera);

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        context: &mut RenderContext<'w>,
        (target, OverlayCamera): (&'w ViewTarget, &'w OverlayCamera),
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some((frame, shown)) = world
            .get_resource::<ExtractedOverlay>()
            .and_then(|frame| Some((frame, frame.shown?)))
        else {
            return Ok(());
        };
        // When the install failed, the error handler of the app has received the error, and the overlay draws nothing.
        let Some(overlay) = world.get_non_send_resource::<RefCell<GpuOverlay>>() else {
            return Ok(());
        };
        let mut overlay = overlay.borrow_mut();
        let texture = target.main_texture();
        let queue = world.resource::<RenderQueue>();
        let signaled = borrowed_image(texture).and_then(|image| {
            signal_on_queue(queue, overlay.gpu.ready_semaphore()).map(|()| image)
        });
        let image = match signaled {
            Ok(image) => image,
            Err(error) => {
                overlay.failed(&error);
                return Ok(());
            }
        };
        let host_started = Instant::now();
        // This transition gives the texture a state tracked by wgpu before the external draw. Later
        // encoders then record the barrier for reading the texture themselves.
        context.command_encoder().transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &**texture,
                selector: None,
                state: wgpu::TextureUses::COLOR_TARGET,
            }),
        );
        let pending = std::mem::replace(
            context,
            RenderContext::new(context.render_device().clone(), None),
        );
        let (commands, device, diagnostics) = pending.finish();
        let host_command_buffers = commands.len();
        *context = RenderContext::new(device, diagnostics);
        let _submission_ordered_by_queue = queue.submit(commands);
        let host_flush = host_started.elapsed();
        let started = Instant::now();
        let (texture_before, view_before) = (texture.id(), target.main_texture_view().id());
        let GpuOverlay { gpu, painter, .. } = &mut *overlay;
        // SAFETY: `draw` needs promises about the state of an image of the host and about the order of the queue,
        // because no type of wgpu proves them. The embedded API of lavashell-renderer has no safe draw into a borrowed
        // image. The image is the main texture of the view, which exists for the whole frame. The submission just
        // before signals the ready semaphore and leaves the image in COLOR_TARGET, the state declared by
        // `borrowed_image`. The next submission of wgpu runs after the draw on the same queue, as the contract of
        // `LavashellOverlayPlugin::new` requires. Skia returns the image in the layout that the tracker of wgpu
        // expects.
        let drawn = unsafe {
            gpu.draw(image, |canvas| {
                painter.paint_scene(canvas, frame.snapshot.at(shown.presentation), shown.density);
            })
        };
        match drawn {
            Ok(()) => {
                let image = OverlayImageTrace {
                    texture_before,
                    texture_after: target.main_texture().id(),
                    view_before,
                    view_after: target.main_texture_view().id(),
                    output_view: target.out_texture().id(),
                    size: (texture.width(), texture.height()),
                    format: texture.format(),
                    host_command_buffers,
                    overlay_submission: overlay.sequence,
                };
                overlay.submitted(OverlayTiming {
                    sequence: frame.sequence,
                    submit: started.elapsed(),
                    host_flush,
                    image,
                });
            }
            Err(error) => overlay.failed(&Error::OverlayDraw(error)),
        }
        context.command_encoder().transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &**texture,
                selector: None,
                state: wgpu::TextureUses::RESOURCE,
            }),
        );
        Ok(())
    }
}

/// The Vulkan image of `texture`, described for a draw by the overlay after Bevy has signaled the ready semaphore.
/// Fails with the reason when the overlay cannot draw into the image.
fn borrowed_image(texture: &wgpu::Texture) -> Result<BorrowedImage, Error> {
    let image = BorrowedImage {
        image: image_of(texture)?,
        size: (texture.width(), texture.height()),
        format: vulkan_format(texture.format())?,
        usage: vulkan_usage(texture.usage())?,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        return_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ready: ReadySignal::Signaled,
    };
    image.check().map_err(Error::OverlayTarget)?;
    Ok(image)
}
