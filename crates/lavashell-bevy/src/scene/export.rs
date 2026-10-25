// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CompletedFrame, SCENE_FORMAT, SCENE_USAGE, SceneBridge,
    bridge::{Lease, ReturnTarget},
    producer::{ScenePublication, SceneSize, Selection, Targets},
    transfer::{self, Transfer},
};
use crate::Error;
use crate::texture_types::image_description;
use crate::vulkan_host::wrap_external_image;
use ash::vk;
use bevy::{
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_resource::Texture,
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use lavashell_renderer::vulkan::{CompletedRelease, PendingRelease, ReleaseProgress};
use lavashell_vulkan::{Commands, Device, ExportableImage, Image, Status};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

type TargetId = bevy::asset::AssetId<bevy::image::Image>;

struct Slot {
    image: ExportableImage,
    setup: Transfer,
    texture: Option<Texture>,
    release: Release,
}

/// How far the image of a slot is in its transfer to the consumer.
enum Release {
    /// The image belongs to the queue of Bevy. The commands record the next release of the image.
    Ready(Commands),
    /// The release of a captured scene, submitted to the queue.
    Pending(PendingExport),
    /// The image belongs to the queue family of the consumer. The commands record the next release of the image.
    /// `Slot::installed` acquires the image back before its next render.
    Released(Commands),
}

/// The release of a captured scene, with the data published for the scene once the release completes.
struct PendingExport {
    release: PendingRelease,
    scene: CapturedScene,
}

struct CapturedScene {
    generation: u64,
    at: Instant,
    publish: bool,
}

impl Slot {
    /// The export state of a target of `size`, with its image allocated and its clear submitted.
    fn open(device: &Arc<Device>, size: SceneSize) -> Result<Self, Error> {
        let extent = vk::Extent2D {
            width: size.width.get(),
            height: size.height.get(),
        };
        let description = image_description(SCENE_FORMAT, SCENE_USAGE, extent)?;
        let image = Image::exportable(device, &description).map_err(Error::SceneImage)?;
        let commands = || Commands::new(device).map_err(Error::TransferCommands);
        Ok(Self {
            setup: Transfer::new(image.image().clone(), commands()?).initialize()?,
            release: Release::Ready(commands()?),
            texture: None,
            image,
        })
    }

    /// The slot with its texture, which is created once the clear of the image is complete. When the target is
    /// `selected` after a release, the image is also acquired back from the consumer.
    fn installed(self, device: &RenderDevice, selected: bool) -> Result<Self, Error> {
        let Self {
            image,
            setup,
            mut texture,
            release,
        } = self;
        if texture.is_none() && setup.status()? == Status::Complete {
            // SAFETY: The texture is created only after the setup that cleared the image is complete, as the status
            // check above confirms. The contents of the image are therefore initialized.
            let wrapped =
                unsafe { wrap_external_image(device, &image, SCENE_FORMAT, SCENE_USAGE) }?;
            texture = Some(wrapped);
        }
        let (setup, release) = match release {
            Release::Released(commands) if selected => (setup.acquire()?, Release::Ready(commands)),
            release => (setup, release),
        };
        Ok(Self {
            image,
            setup,
            texture,
            release,
        })
    }

    /// The slot with the release of its image to the consumer submitted, after a render of `scene` was copied out of
    /// the image.
    fn released(self, returned: ReturnTarget, scene: CapturedScene) -> Result<Self, Error> {
        let Release::Ready(commands) = self.release else {
            return Err(Error::PendingTransfer);
        };
        returned.exported();
        let lease = Lease::new(returned, self.image.image().clone());
        let release = transfer::release(&self.image, commands, lease)?;
        Ok(Self {
            release: Release::Pending(PendingExport { release, scene }),
            ..self
        })
    }

    /// The slot with the scene of a complete release published, without waiting for a release that is pending.
    fn collected(self, bridge: &SceneBridge) -> Result<Self, Error> {
        let Release::Pending(PendingExport { release, scene }) = self.release else {
            return Ok(self);
        };
        let release = match release.poll() {
            Ok(ReleaseProgress::Pending(release)) => {
                Release::Pending(PendingExport { release, scene })
            }
            Ok(ReleaseProgress::Complete(completed)) => published(bridge, scene, completed),
            Err(error) => return Err(release_failed(bridge, error)),
        };
        Ok(Self { release, ..self })
    }

    /// The slot with the scene of its release published once the release is complete.
    fn drained(self, bridge: &SceneBridge) -> Result<Self, Error> {
        let Release::Pending(PendingExport { release, scene }) = self.release else {
            return Ok(self);
        };
        let completed = release
            .wait()
            .map_err(|error| release_failed(bridge, error))?;
        Ok(Self {
            release: published(bridge, scene, completed),
            ..self
        })
    }
}

/// Publishes the scene of a complete release, or counts the scene as unready when `publish` is false. Returns
/// `Release::Released` with the commands of the release, for the next release.
fn published(
    bridge: &SceneBridge,
    CapturedScene {
        generation,
        at,
        publish,
    }: CapturedScene,
    CompletedRelease { image, commands }: CompletedRelease,
) -> Release {
    if publish {
        bridge.publish(CompletedFrame::new(generation, image, at));
    } else {
        // The withheld image is dropped when this function returns. Dropping the image drops its lease, which returns
        // the target to the ring.
        bridge.finish_unready();
    }
    Release::Released(commands)
}

/// Counts the scene of a failed release as unready.
fn release_failed(bridge: &SceneBridge, error: lavashell_renderer::Error) -> Error {
    bridge.finish_unready();
    Error::ExportScene(error)
}

/// The export state of every scene target, on the device of Bevy.
#[derive(Resource)]
pub(super) struct Exports {
    device: Arc<Device>,
    slots: BTreeMap<TargetId, Slot>,
}

impl Exports {
    pub fn new(device: Arc<Device>) -> Self {
        Self {
            device,
            slots: BTreeMap::new(),
        }
    }

    /// Applies `step` to every slot until a step fails. The slots after the failed step remain unchanged.
    fn advance_all(
        &mut self,
        mut step: impl FnMut(Slot) -> Result<Slot, Error>,
    ) -> Result<(), Error> {
        let mut outcome = Ok(());
        self.slots = std::mem::take(&mut self.slots)
            .into_iter()
            .filter_map(|(id, slot)| {
                if outcome.is_err() {
                    return Some((id, slot));
                }
                kept(id, step(slot), &mut outcome)
            })
            .collect();
        outcome
    }

    /// Applies `step` to the slot of `id`.
    fn advance(
        &mut self,
        id: TargetId,
        step: impl FnOnce(Slot) -> Result<Slot, Error>,
    ) -> Result<(), Error> {
        let slot = step(self.slots.remove(&id).ok_or(Error::MissingSlot)?)?;
        self.slots.extend([(id, slot)]);
        Ok(())
    }
}

/// The slot of `id` when its step succeeded. A slot whose step failed is dropped, so that `install` opens a new slot
/// for the target. The error of the failed step is written to `outcome`.
fn kept(
    id: TargetId,
    stepped: Result<Slot, Error>,
    outcome: &mut Result<(), Error>,
) -> Option<(TargetId, Slot)> {
    match stepped {
        Ok(slot) => Some((id, slot)),
        Err(error) => {
            *outcome = Err(error);
            None
        }
    }
}

pub(super) fn install(
    targets: Res<Targets>,
    selection: Res<Selection>,
    device: Res<RenderDevice>,
    mut images: ResMut<RenderAssets<GpuImage>>,
    mut exports: ResMut<Exports>,
) -> Result {
    let (targets, device) = (targets.into_inner(), device.into_inner());
    let selected =
        (selection.into_inner().selected.as_ref()).map(|selected| selected.target.image.id());
    let Exports {
        device: export,
        slots,
    } = &mut *exports;
    let mut previous = std::mem::take(slots);
    let mut outcome = Ok(());
    // The slots of targets that left the ring remain in `previous` and are dropped with it.
    *slots = (targets.current.iter().chain(&targets.retired))
        .filter_map(|target| {
            let id = target.image.id();
            let slot = previous.remove(&id);
            if outcome.is_err() {
                return slot.map(|slot| (id, slot));
            }
            let installed = slot
                .map_or_else(|| Slot::open(export, target.size), Ok)
                .and_then(|slot| slot.installed(device, selected == Some(id)));
            kept(id, installed, &mut outcome)
        })
        .collect();
    for (id, slot) in &*slots {
        let Some(texture) = &slot.texture else {
            continue;
        };
        if images
            .get(*id)
            .is_some_and(|image| image.texture.id() == texture.id())
        {
            continue;
        }
        let exported = GpuImage {
            texture: texture.clone(),
            texture_view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture_format: SCENE_FORMAT,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor::default()),
            size: texture.size(),
            mip_level_count: 1,
        };
        let _replaced_image_of_bevy = images.insert(*id, exported);
    }
    outcome?;
    Ok(())
}

pub(super) fn capture(
    selection: Res<Selection>,
    publication: Res<ScenePublication>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    bridge: Res<SceneBridge>,
    mut exports: ResMut<Exports>,
) -> Result {
    let (selection, publication, device, queue, bridge) = (
        selection.into_inner(),
        publication.into_inner(),
        device.into_inner(),
        queue.into_inner(),
        bridge.into_inner(),
    );
    // The selected target returns to the ring when `returned` is dropped, on every path that does not release the
    // scene of the target to the consumer.
    let selected = (selection.selected.as_ref())
        .map(|selected| (selected, ReturnTarget::new(selected.target.state.clone())));
    collect(bridge, &mut exports)?;
    let Some((selected, returned)) = selected else {
        return Ok(());
    };
    let id = selected.target.image.id();
    let Some(texture) = &exports.slots.get(&id).ok_or(Error::MissingSlot)?.texture else {
        bridge.unready();
        return Ok(());
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("scene native interop layout"),
    });
    encoder.transition_resources(
        std::iter::empty(),
        std::iter::once(wgpu::TextureTransition {
            texture: &**texture,
            selector: None,
            state: wgpu::TextureUses::COPY_SRC,
        }),
    );
    let _submission_ordered_by_queue = queue.submit([encoder.finish()]);
    let scene = CapturedScene {
        generation: selected.generation,
        at: selected.at,
        publish: publication.ready,
    };
    exports.advance(id, |slot| slot.released(returned, scene))?;
    bridge.submit();
    Ok(())
}

/// Publishes the scenes whose releases are complete, without waiting for the others.
pub(super) fn collect(bridge: &SceneBridge, exports: &mut Exports) -> Result<(), Error> {
    exports.advance_all(|slot| slot.collected(bridge))
}

/// Waits for every pending release, publishes its scene, and drops the latest published scene.
pub(super) fn drain(bridge: &SceneBridge, exports: &mut Exports) -> Result<(), Error> {
    exports.advance_all(|slot| slot.drained(bridge))?;
    bridge.discard_latest();
    Ok(())
}
