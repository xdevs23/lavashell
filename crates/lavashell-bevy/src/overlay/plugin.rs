// SPDX-License-Identifier: GPL-3.0-only

use super::extract::{ExtractedOverlay, OverlayCamera, extract_overlay, prepare_overlay_targets};
use super::frame::{SentFrame, StartOverlayScene};
use crate::Error;
use crate::texture_types::{vulkan_format, vulkan_usage};
use crate::timings::{OverlayOutcome, OverlayTiming};
use crate::vulkan_host::{SharedDeviceRequest, image_of, signal_on_queue};
use ash::vk;
use bevy::{
    core_pipeline::core_3d::graph::{Core3d, Node3d},
    prelude::*,
    render::{
        ExtractSchedule, RenderApp,
        extract_component::ExtractComponentPlugin,
        render_graph::{
            NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
        },
        renderer::{RenderContext, RenderQueue},
        view::ViewTarget,
    },
};
use lavashell_actor::Latest;
use lavashell_core::error::message_chain;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{BorrowedImage, EmbeddedVulkanRenderer, Fonts, Painter, ReadySignal};
use std::{cell::RefCell, sync::Arc, time::Instant};

pub struct LavashellOverlayPlugin {
    fonts: Fonts,
    records: Arc<Latest<OverlayOutcome>>,
}

impl LavashellOverlayPlugin {
    /// # Safety
    /// Disable `PipelinedRenderingPlugin` and never submit to the `RenderQueue` of Bevy from another
    /// thread. The render schedule must order every submission to that queue before or after the
    /// overlay node, for the lifetime of the app, including shutdown.
    #[must_use]
    pub unsafe fn new(fonts: Fonts, records: Arc<Latest<OverlayOutcome>>) -> Self {
        Self { fonts, records }
    }
}

struct InstallOverlay {
    fonts: Fonts,
    records: Arc<Latest<OverlayOutcome>>,
    shared_device: SharedDeviceRequest,
}

impl Command<Result<(), Error>> for InstallOverlay {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        // SAFETY: No type of wgpu proves that the submissions to a queue are serialized, so opening the shared device
        // needs that promise from the caller and has no safe form. The contract of `LavashellOverlayPlugin::new`
        // forbids submissions from other threads and orders the submissions of Bevy with the overlay node. Opening
        // the device refuses an app with pipelined rendering.
        let device = unsafe { self.shared_device.open(world) }?;
        // SAFETY: `EmbeddedVulkanRenderer::new` needs the promise that the queue of the device is serialized, because
        // no type of wgpu proves it. The embedded API of lavashell-renderer has no safe constructor for a device
        // created by a host. The queue supports graphics, because wgpu draws on it. The contract of
        // `LavashellOverlayPlugin::new` serializes the submissions of Bevy with every call of the renderer. The
        // renderer is called only in the overlay node and when it is dropped.
        let gpu =
            unsafe { EmbeddedVulkanRenderer::new(&device) }.map_err(Error::OverlayRenderer)?;
        let painter = gpu.painter(&self.fonts).map_err(Error::OverlayPainter)?;
        world.insert_non_send_resource(RefCell::new(GpuOverlay {
            painter,
            gpu,
            records: self.records,
        }));
        Ok(())
    }
}

impl Plugin for LavashellOverlayPlugin {
    fn build(&self, app: &mut App) {
        let world = app.world_mut();
        world.commands().queue(StartOverlayScene);
        world.flush();
        let _app_returned_for_chaining = app
            .add_plugins(ExtractComponentPlugin::<OverlayCamera>::default())
            .add_systems(PostUpdate, prepare_overlay_targets);
        let _render_app_returned_for_chaining = app
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
        let install = InstallOverlay {
            fonts: self.fonts.clone(),
            records: Arc::clone(&self.records),
            shared_device: SharedDeviceRequest::of(app),
        };
        let world = app.sub_app_mut(RenderApp).world_mut();
        world.commands().queue(install);
        world.flush();
    }
}

struct GpuOverlay {
    painter: Painter,
    gpu: EmbeddedVulkanRenderer,
    records: Arc<Latest<OverlayOutcome>>,
}

impl GpuOverlay {
    fn resolve_and_signal(
        &self,
        texture: &wgpu::Texture,
        queue: &RenderQueue,
    ) -> Result<BorrowedImage, Error> {
        let image = borrowed_image(texture)?;
        signal_on_queue(queue, self.gpu.ready_semaphore())?;
        Ok(image)
    }

    fn draw(
        &mut self,
        image: BorrowedImage,
        frame: &ExtractedOverlay,
        sent: SentFrame,
    ) -> Result<(), Error> {
        let Self { gpu, painter, .. } = self;
        // SAFETY: `draw` needs promises about the state of an image of the host and about the order of the queue,
        // because no type of wgpu proves them. The embedded API of lavashell-renderer has no safe draw into a borrowed
        // image. The image is the main texture of the view, which exists for the whole frame. The submission just
        // before signals the ready semaphore and leaves the image in COLOR_TARGET, the state declared by
        // `borrowed_image`. The next submission of wgpu runs after the draw on the same queue, as the contract of
        // `LavashellOverlayPlugin::new` requires. Skia returns the image in the layout that the tracker of wgpu
        // expects.
        unsafe {
            gpu.draw(image, |canvas| {
                painter.paint_scene(
                    canvas,
                    frame.surface,
                    frame.built.scene.at(sent.presentation),
                    sent.density,
                )
            })
        }
        .and_then(|painted| painted)
        .map_err(Error::OverlayDraw)
    }

    fn report(&self, sequence: u64, result: Result<OverlayTiming, Error>) {
        if let Err(error) = &result {
            log::error!(
                "the Lavashell overlay was not drawn: {}",
                message_chain(error)
            );
        }
        self.records.publish(OverlayOutcome { sequence, result });
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
        let Some((frame, sent)) = world
            .get_resource::<ExtractedOverlay>()
            .and_then(|frame| Some((frame, frame.sent?)))
        else {
            return Ok(());
        };
        let Some(overlay) = world.get_non_send_resource::<RefCell<GpuOverlay>>() else {
            return Ok(());
        };
        let mut overlay = overlay.borrow_mut();
        let texture = target.main_texture();
        let queue = world.resource::<RenderQueue>();
        let image = match overlay.resolve_and_signal(texture, queue) {
            Ok(image) => image,
            Err(error) => {
                overlay.report(sent.sequence, Err(error));
                return Ok(());
            }
        };
        let host_started = Instant::now();
        flush_host_commands(context, texture, queue);
        let host_flush = host_started.elapsed();
        let started = Instant::now();
        let drawn = overlay.draw(image, frame, sent).map(|()| OverlayTiming {
            submit: started.elapsed(),
            host_flush,
        });
        overlay.report(sent.sequence, drawn);
        transition_texture(context, texture, wgpu::TextureUses::RESOURCE);
        Ok(())
    }
}

fn flush_host_commands(
    context: &mut RenderContext<'_>,
    texture: &wgpu::Texture,
    queue: &RenderQueue,
) {
    transition_texture(context, texture, wgpu::TextureUses::COLOR_TARGET);
    let pending = std::mem::replace(
        context,
        RenderContext::new(context.render_device().clone(), None),
    );
    let (commands, device, diagnostics) = pending.finish();
    *context = RenderContext::new(device, diagnostics);
    let _submission_ordered_by_queue = queue.submit(commands);
}

fn transition_texture(
    context: &mut RenderContext<'_>,
    texture: &wgpu::Texture,
    state: wgpu::TextureUses,
) {
    context.command_encoder().transition_resources(
        std::iter::empty(),
        std::iter::once(wgpu::TextureTransition {
            texture,
            selector: None,
            state,
        }),
    );
}

fn borrowed_image(texture: &wgpu::Texture) -> Result<BorrowedImage, Error> {
    let image = BorrowedImage {
        image: image_of(texture)?,
        size: PixelSize::new(texture.width(), texture.height()),
        format: vulkan_format(texture.format())?,
        usage: vulkan_usage(texture.usage())?,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        return_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ready: ReadySignal::Signaled,
    };
    image.check().map_err(Error::OverlayTarget)?;
    Ok(image)
}
