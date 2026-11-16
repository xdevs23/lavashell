// SPDX-License-Identifier: GPL-3.0-only
use super::{
    SCENE_FORMAT, SCENE_USAGE, SceneBridge,
    bridge::{TargetPhase, TargetState},
    export::{self, Exports},
};
use crate::Error;
use crate::vulkan_host::SharedDeviceRequest;
use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
    },
};
use std::{num::NonZeroU32, sync::Arc, time::Instant};

#[derive(Component)]
pub struct SceneCamera;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSize {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

#[derive(Resource)]
pub struct ScenePublication {
    pub ready: bool,
}

impl Default for ScenePublication {
    fn default() -> Self {
        Self { ready: true }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SceneExportSet;

#[derive(Clone)]
pub(super) struct Target {
    pub image: Handle<Image>,
    pub size: SceneSize,
    pub state: Arc<TargetState>,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub(super) struct Targets {
    pub current: Vec<Target>,
    pub retired: Vec<Target>,
}

impl Targets {
    fn free_target(&mut self, open: impl FnOnce() -> Target) -> Target {
        let (free, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.current)
            .into_iter()
            .partition(|target| target.state.get() == TargetPhase::Free);
        let target = free.into_iter().next().unwrap_or_else(open);
        self.current = held;
        self.current.push(target.clone());
        target
    }
}

#[derive(Clone)]
pub(super) struct Selected {
    pub target: Target,
    pub generation: u64,
    pub at: Instant,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub(super) struct Selection {
    pub selected: Option<Selected>,
}

pub struct SceneProducerPlugin {
    bridge: SceneBridge,
    size: SceneSize,
}

impl SceneProducerPlugin {
    /// # Safety
    /// Disable `PipelinedRenderingPlugin`. Submit to the `RenderQueue` of Bevy only from the render graph and from
    /// systems ordered before or after `RenderSystems::ManageViews` and `SceneExportSet`, so that they never run at
    /// the same time as these sets. Never submit from another thread. The export submits to the same queue in those
    /// sets for the lifetime of the app, including shutdown.
    #[must_use]
    pub unsafe fn new(bridge: SceneBridge, size: SceneSize) -> Self {
        Self { bridge, size }
    }
}

impl Plugin for SceneProducerPlugin {
    fn build(&self, app: &mut App) {
        let _chained_app = app
            .insert_resource(self.bridge.clone())
            .insert_resource(self.size)
            .init_resource::<Targets>()
            .init_resource::<Selection>()
            .add_plugins((
                ExtractResourcePlugin::<Targets>::default(),
                ExtractResourcePlugin::<Selection>::default(),
            ))
            .add_systems(PostUpdate, rotate.before(bevy::camera::CameraUpdateSystems));
        let _chained_render_app = app
            .sub_app_mut(RenderApp)
            .insert_resource(self.bridge.clone())
            .init_resource::<ScenePublication>()
            .configure_sets(Render, RenderSystems::Render.run_if(render_ready))
            .configure_sets(
                Render,
                SceneExportSet
                    .after(RenderSystems::Render)
                    .before(RenderSystems::Cleanup)
                    .run_if(render_ready),
            )
            .add_systems(
                Render,
                (
                    export::install
                        .in_set(RenderSystems::ManageViews)
                        .after(
                            bevy::render::render_asset::prepare_assets::<
                                bevy::render::texture::GpuImage,
                            >,
                        )
                        .before(bevy::render::view::prepare_view_attachments),
                    export::capture.in_set(SceneExportSet),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let install = InstallExports {
            shared_device: SharedDeviceRequest::of(app),
        };
        let _replaced_by_the_wall_clock = app
            .world_mut()
            .remove_resource::<bevy::time::TimeReceiver>();
        let world = app.sub_app_mut(RenderApp).world_mut();
        world.commands().queue(install);
        world.flush();
    }
}

struct InstallExports {
    shared_device: SharedDeviceRequest,
}

impl Command<Result<(), Error>> for InstallExports {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        // SAFETY: No type of wgpu proves that the submissions to a queue are serialized, so opening the shared device
        // needs that promise from the caller and has no safe form. The contract of `SceneProducerPlugin::new`
        // serializes the submissions of Bevy with the submissions of the export. Opening the device refuses an app
        // with pipelined rendering.
        let device = unsafe { self.shared_device.open(world) }?;
        world.insert_resource(Exports::new(device));
        Ok(())
    }
}

fn render_ready(selection: Res<Selection>, bridge: Res<SceneBridge>) -> bool {
    selection.into_inner().selected.is_some() && !bridge.into_inner().has_export_in_flight()
}

fn new_target(images: &mut Assets<Image>, size: SceneSize) -> Target {
    let mut image = Image::new_target_texture(size.width.get(), size.height.get(), SCENE_FORMAT);
    image.texture_descriptor.usage |= SCENE_USAGE;
    Target {
        image: images.add(image),
        size,
        state: TargetState::new(),
    }
}

fn rotate(
    size: Res<SceneSize>,
    bridge: Res<SceneBridge>,
    mut targets: ResMut<Targets>,
    mut selection: ResMut<Selection>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Camera, With<SceneCamera>>,
) {
    let (size, bridge) = (*size.into_inner(), bridge.into_inner());
    selection.selected = None;
    if cameras.is_empty() {
        return;
    }
    if bridge.has_export_in_flight() {
        bridge.count_skipped_busy();
        return;
    }
    if (targets.retired.iter()).all(|t| t.state.get() == TargetPhase::Free) {
        targets.retired.clear();
    }
    let resized = targets.current.iter().any(|t| t.size != size);
    if resized && targets.retired.is_empty() {
        targets.retired = std::mem::take(&mut targets.current);
    }
    let target = targets.free_target(|| new_target(&mut images, size));
    target.state.set(TargetPhase::Rendering);
    for mut camera in &mut cameras {
        camera.is_active = true;
        camera.target = RenderTarget::Image(target.image.clone().into());
    }
    selection.selected = Some(Selected {
        target,
        generation: bridge.next_generation(),
        at: Instant::now(),
    });
}

pub fn poll_app(app: &mut App) -> Result<(), Error> {
    with_exports(app, export::collect)
}

pub fn drain_app(app: &mut App) -> Result<(), Error> {
    with_exports(app, export::drain)
}

fn with_exports(
    app: &mut App,
    work: fn(&SceneBridge, &mut Exports) -> Result<(), Error>,
) -> Result<(), Error> {
    let world = (app.get_sub_app_mut(RenderApp))
        .ok_or(Error::RenderAppMissing)?
        .world_mut();
    let bridge = (world.get_resource::<SceneBridge>().cloned()).ok_or(Error::SceneBridgeMissing)?;
    let exports = world
        .get_resource_mut::<Exports>()
        .ok_or(Error::ExportsMissing)?;
    work(&bridge, exports.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    fn scene_size() -> Result<SceneSize, TestError> {
        Ok(SceneSize {
            width: NonZeroU32::new(640).ok_or(TestError::ZeroSize)?,
            height: NonZeroU32::new(360).ok_or(TestError::ZeroSize)?,
        })
    }

    fn world(size: SceneSize, bridge: &SceneBridge, targets: Targets) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(size);
        world.insert_resource(bridge.clone());
        world.insert_resource(targets);
        world.insert_resource(Selection::default());
        world.insert_resource(Assets::<Image>::default());
        let camera = world.spawn((Camera::default(), SceneCamera)).id();
        (world, camera)
    }

    fn rotate_targets(world: &mut World) -> Result<(), TestError> {
        world.run_system_cached(rotate)?;
        Ok(())
    }

    fn target(size: SceneSize, phase: TargetPhase) -> Target {
        let target = Target {
            image: Handle::default(),
            size,
            state: TargetState::new(),
        };
        target.state.set(phase);
        target
    }

    #[test]
    fn incomplete_scene_keeps_preparation_active_but_cannot_render() -> Result<(), TestError> {
        let bridge = SceneBridge::default();
        bridge.begin_export();
        let size = scene_size()?;
        let free = target(size, TargetPhase::Free);
        let targets = Targets {
            current: vec![free.clone()],
            retired: Vec::new(),
        };
        let (mut world, camera) = world(size, &bridge, targets);
        rotate_targets(&mut world)?;
        assert!(
            world
                .get::<Camera>(camera)
                .ok_or(TestError::NoCamera)?
                .is_active
        );
        assert!(world.resource::<Selection>().selected.is_none());
        assert!(!world.run_system_cached(render_ready)?);
        assert_eq!(free.state.get(), TargetPhase::Free);
        assert!(bridge.has_export_in_flight());
        assert_eq!(bridge.stats().skipped_busy, 1);
        world.resource_mut::<Selection>().selected = Some(Selected {
            target: free,
            generation: 0,
            at: Instant::now(),
        });
        assert!(!world.run_system_cached(render_ready)?);
        world.insert_resource(SceneBridge::default());
        assert!(world.run_system_cached(render_ready)?);
        Ok(())
    }

    #[test]
    fn the_ring_starts_with_one_target() -> Result<(), TestError> {
        let (mut world, _camera_is_not_checked) =
            world(scene_size()?, &SceneBridge::default(), Targets::default());
        rotate_targets(&mut world)?;
        let targets = world.resource::<Targets>();
        assert_eq!(targets.current.len(), 1);
        let selected =
            (world.resource::<Selection>().selected.clone()).ok_or(TestError::NoSelection)?;
        assert_eq!(selected.target.state.get(), TargetPhase::Rendering);
        assert!(
            targets
                .current
                .iter()
                .any(|t| t.image == selected.target.image)
        );
        Ok(())
    }

    #[test]
    fn a_consumer_that_holds_every_target_gets_one_more() -> Result<(), TestError> {
        let size = scene_size()?;
        let targets = Targets {
            current: vec![target(size, TargetPhase::Exported)],
            retired: Vec::new(),
        };
        let (mut world, _camera_is_not_checked) = world(size, &SceneBridge::default(), targets);
        rotate_targets(&mut world)?;
        let selected =
            (world.resource::<Selection>().selected.clone()).ok_or(TestError::NoSelection)?;
        assert_eq!(selected.target.state.get(), TargetPhase::Rendering);
        assert_eq!(world.resource::<Targets>().current.len(), 2);
        Ok(())
    }

    #[test]
    fn targets_the_consumer_returned_are_retired_down_to_one() -> Result<(), TestError> {
        let size = scene_size()?;
        let targets = Targets {
            current: vec![
                target(size, TargetPhase::Exported),
                target(size, TargetPhase::Free),
                target(size, TargetPhase::Free),
            ],
            retired: Vec::new(),
        };
        let (mut world, _camera_is_not_checked) = world(size, &SceneBridge::default(), targets);
        rotate_targets(&mut world)?;
        assert_eq!(world.resource::<Targets>().current.len(), 2);
        Ok(())
    }
}
