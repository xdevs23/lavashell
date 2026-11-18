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
use lavashell_vulkan::{Commands, Device, ExportableImage, Image, Status};
use lavashell_vulkan_renderer::vulkan::{CompletedRelease, PendingRelease, ReleaseProgress};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

type TargetId = bevy::asset::AssetId<bevy::image::Image>;

struct Slot {
    image: ExportableImage,
    setup: Transfer,
    texture: Option<Texture>,
    release: Release,
}

enum Release {
    Ready(Commands),
    Pending(PendingExport),
    Released(Commands),
}

struct PendingExport {
    release: PendingRelease,
    scene: CapturedScene,
}

struct CapturedScene {
    generation: u64,
    at: Instant,
    delivery: Delivery,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Publish,
    Withhold,
}

impl From<bool> for Delivery {
    fn from(ready: bool) -> Self {
        if ready { Self::Publish } else { Self::Withhold }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Selected,
    Unselected,
}

impl From<bool> for Choice {
    fn from(selected: bool) -> Self {
        if selected {
            Self::Selected
        } else {
            Self::Unselected
        }
    }
}

impl Slot {
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

    fn install(self, device: &RenderDevice, choice: Choice) -> Result<Self, Error> {
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
            Release::Released(commands) if choice == Choice::Selected => {
                (setup.acquire()?, Release::Ready(commands))
            }
            release => (setup, release),
        };
        Ok(Self {
            image,
            setup,
            texture,
            release,
        })
    }

    fn release(self, returned: ReturnTarget, scene: CapturedScene) -> Result<Self, Error> {
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

    fn collect(self, bridge: &SceneBridge) -> Result<Self, Error> {
        let Release::Pending(PendingExport { release, scene }) = self.release else {
            return Ok(self);
        };
        let release = match release.poll() {
            Ok(ReleaseProgress::Pending(release)) => {
                Release::Pending(PendingExport { release, scene })
            }
            Ok(ReleaseProgress::Complete(completed)) => complete_export(bridge, scene, completed),
            Err(error) => return Err(end_failed_export(bridge, error)),
        };
        Ok(Self { release, ..self })
    }

    fn drain(self, bridge: &SceneBridge) -> Result<Self, Error> {
        let Release::Pending(PendingExport { release, scene }) = self.release else {
            return Ok(self);
        };
        let completed = release
            .wait()
            .map_err(|error| end_failed_export(bridge, error))?;
        Ok(Self {
            release: complete_export(bridge, scene, completed),
            ..self
        })
    }
}

fn complete_export(
    bridge: &SceneBridge,
    CapturedScene {
        generation,
        at,
        delivery,
    }: CapturedScene,
    CompletedRelease { image, commands }: CompletedRelease,
) -> Release {
    match delivery {
        Delivery::Publish => bridge.publish(CompletedFrame::new(generation, image, at)),
        Delivery::Withhold => bridge.finish_export_unpublished(),
    }
    Release::Released(commands)
}

fn end_failed_export(bridge: &SceneBridge, error: lavashell_vulkan_renderer::Error) -> Error {
    bridge.finish_export_unpublished();
    Error::ExportScene(error)
}

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

    fn advance_all(
        &mut self,
        mut step: impl FnMut(Slot) -> Result<Slot, Error>,
    ) -> Result<(), Error> {
        let mut advanced = BTreeMap::new();
        let mut waiting = std::mem::take(&mut self.slots).into_iter();
        let outcome: Result<(), Error> = waiting.by_ref().try_for_each(|(id, slot)| {
            advanced.extend([(id, step(slot)?)]);
            Ok(())
        });
        advanced.extend(waiting);
        self.slots = advanced;
        outcome
    }

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
    let mut installed = BTreeMap::new();
    let mut waiting = targets.current.iter().chain(&targets.retired);
    let outcome: Result<(), Error> = waiting.by_ref().try_for_each(|target| {
        let id = target.image.id();
        let slot = (previous.remove(&id)).map_or_else(|| Slot::open(export, target.size), Ok)?;
        let choice = Choice::from(selected == Some(id));
        installed.extend([(id, slot.install(device, choice)?)]);
        Ok(())
    });
    installed.extend(waiting.filter_map(|target| {
        let id = target.image.id();
        previous.remove(&id).map(|slot| (id, slot))
    }));
    *slots = installed;
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
    let selected = (selection.selected.as_ref())
        .map(|selected| (selected, ReturnTarget::new(selected.target.state.clone())));
    collect(bridge, &mut exports)?;
    let Some((selected, returned)) = selected else {
        return Ok(());
    };
    let id = selected.target.image.id();
    let Some(texture) = &exports.slots.get(&id).ok_or(Error::MissingSlot)?.texture else {
        bridge.count_skipped_unready();
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
        delivery: Delivery::from(publication.ready),
    };
    exports.advance(id, |slot| slot.release(returned, scene))?;
    bridge.begin_export();
    Ok(())
}

pub(super) fn collect(bridge: &SceneBridge, exports: &mut Exports) -> Result<(), Error> {
    exports.advance_all(|slot| slot.collect(bridge))
}

pub(super) fn drain(bridge: &SceneBridge, exports: &mut Exports) -> Result<(), Error> {
    exports.advance_all(|slot| slot.drain(bridge))?;
    bridge.discard_latest();
    Ok(())
}
