// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::ApplyError;
use lavashell_renderer::Error as RendererError;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the render device of Bevy is not a Vulkan device")]
    NotVulkanDevice,
    #[error("the render queue of Bevy is not a Vulkan queue")]
    NotVulkanQueue,
    #[error("the texture is not a Vulkan image")]
    NotVulkanTexture,
    #[error("the texture format {0:?} has no Vulkan format that Lavashell draws into")]
    TextureFormat(wgpu::TextureFormat),
    #[error("the texture usage {0:?} has no Vulkan image usage")]
    UnsupportedUsage(wgpu::TextureUsages),
    #[error("the texture does not describe the Vulkan image it wraps")]
    TextureDescription,
    #[error("the overlay renderer cannot start on the device of Bevy")]
    OverlayRenderer(#[source] RendererError),
    #[error("the overlay painter cannot start")]
    OverlayPainter(#[source] RendererError),
    #[error("the overlay cannot draw into the image of the camera")]
    OverlayTarget(#[source] RendererError),
    #[error("the overlay cannot draw")]
    OverlayDraw(#[source] RendererError),
    #[error("the choreographer or the scene builder of the overlay cannot start")]
    StartScene(#[source] lavashell_runtime::scene_pipeline::Error),
    #[error("the surface of the overlay could not get an id")]
    NumberOverlaySurface(#[source] lavashell_core::id_counter::IdsExhausted),
    #[error("cannot create an exportable scene image")]
    SceneImage(#[source] lavashell_vulkan::Error),
    #[error("cannot create the commands that hand a scene image over")]
    TransferCommands(#[source] lavashell_vulkan::Error),
    #[error("cannot clear a new scene image")]
    InitializeScene(#[source] lavashell_vulkan::Error),
    #[error("cannot record the release of a scene image to its consumer")]
    ReleaseScene(#[source] lavashell_vulkan::Error),
    #[error("cannot submit the release of a scene image to its consumer")]
    SubmitRelease(#[source] RendererError),
    #[error("cannot take a scene image back from its consumer")]
    AcquireScene(#[source] lavashell_vulkan::Error),
    #[error("cannot read whether the hand-over of a scene image is complete")]
    TransferStatus(#[source] lavashell_vulkan::Error),
    #[error("cannot take back the commands of a finished hand-over of a scene image")]
    ReclaimTransfer(#[source] lavashell_vulkan::Error),
    #[error("the hand-over of a scene image is recorded again while the previous one is pending")]
    PendingTransfer,
    #[error("cannot export a released scene image")]
    ExportScene(#[source] RendererError),
    #[error("the selected scene target has no export slot")]
    MissingSlot,
    #[error("the app does not contain a render sub-app, so scene export cannot run")]
    RenderAppMissing,
    #[error(
        "the render sub-app does not contain a scene bridge, so the scene producer plugin was not built"
    )]
    SceneBridgeMissing,
    #[error("scene export is not installed in the render sub-app")]
    ExportsMissing,
    #[error("Bevy renders pipelined, so the overlay and the scene export cannot share its queue")]
    PipelinedRendering,
}

#[derive(Debug, thiserror::Error)]
pub enum ShowFrameError {
    #[error("a change in the frame of the view does not fit the scene shown before")]
    Mirror(#[source] ApplyError),
    #[error("the view painted a leaf whose commands do not make a packet")]
    Packet(#[source] UnbalancedRestore),
    #[error("every id of a scene node of the overlay is used")]
    NumberSceneNode(#[source] IdsExhausted),
    #[error("the choreographer or the scene builder of the overlay failed")]
    Scene(#[source] Arc<lavashell_runtime::scene_pipeline::Error>),
    #[error("the choreographer and the scene builder of the overlay stopped")]
    SceneStopped,
}
