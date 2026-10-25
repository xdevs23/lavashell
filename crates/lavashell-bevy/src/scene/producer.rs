// SPDX-License-Identifier: GPL-3.0-only
use super::{
    SCENE_FORMAT, SCENE_USAGE, SceneBridge,
    bridge::{SlotState, TargetState},
    export::{self, Exports},
};
use crate::Error;
use crate::vulkan_host::shared_device;
use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        pipelined_rendering::PipelinedRenderingPlugin,
    },
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Instant,
};

#[derive(Component)]
pub struct SceneCamera;

/// The size of a scene image in pixels.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSize {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

/// The most scenes that the consumer reads at once.
#[derive(Resource, Clone, Copy)]
struct FramesInFlight(NonZeroUsize);

/// Whether the export publishes the rendered scenes. Set `ready` to false while the scene initializes, so that the
/// consumer receives no scene from that time.
#[derive(Resource)]
pub struct ScenePublication {
    pub ready: bool,
}

impl Default for ScenePublication {
    fn default() -> Self {
        Self { ready: true }
    }
}

/// The system set of the render world in which the export captures the rendered scene. Systems that set
/// `ScenePublication::ready` for the current run of the render graph run before this set.
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

/// The target that the next render exports, with the generation of the scene and the time of the selection.
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

/// Renders the `SceneCamera` cameras of an app into a ring of scene targets and exports each rendered scene. The
/// render graph of the `RenderApp` runs only while a target is selected and no export is pending. All cameras of the
/// app render into the same target. Use a separate app for another host.
pub struct SceneProducerPlugin {
    bridge: SceneBridge,
    size: SceneSize,
    frames_in_flight: FramesInFlight,
}

impl SceneProducerPlugin {
    /// Exports scenes of `size` to a consumer that reads at most `frames_in_flight` of them at once, such as the
    /// frames in flight of the swapchain that shows them.
    ///
    /// # Safety
    /// Disable `PipelinedRenderingPlugin`. Submit to the `RenderQueue` of Bevy only from the render graph and from
    /// systems ordered before or after `RenderSystems::ManageViews` and `SceneExportSet`, so that they never run at
    /// the same time as these sets. Never submit from another thread. The export submits to the same queue in those
    /// sets for the lifetime of the app, including shutdown.
    #[must_use]
    pub unsafe fn new(
        bridge: SceneBridge,
        size: SceneSize,
        frames_in_flight: NonZeroUsize,
    ) -> Self {
        Self {
            bridge,
            size,
            frames_in_flight: FramesInFlight(frames_in_flight),
        }
    }
}

impl Plugin for SceneProducerPlugin {
    fn build(&self, app: &mut App) {
        let _chained_app = app
            .insert_resource(self.bridge.clone())
            .insert_resource(self.size)
            .insert_resource(self.frames_in_flight)
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
            pipelined: app.is_plugin_added::<PipelinedRenderingPlugin>(),
        };
        // The render graph runs independently of the updates of the main world. The render world
        // therefore uses the wall clock in place of the time sent by the main world for every render.
        let _replaced_by_the_wall_clock = app
            .world_mut()
            .remove_resource::<bevy::time::TimeReceiver>();
        // A plugin cannot return an error from `finish`, so the install runs as a command. Bevy passes an error of the
        // command to the error handler of the app.
        let world = app.sub_app_mut(RenderApp).world_mut();
        world.commands().queue(install);
        world.flush();
    }
}

/// Installs the export state into the render world, unless Bevy renders pipelined.
struct InstallExports {
    pipelined: bool,
}

impl Command<Result<(), Error>> for InstallExports {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        if self.pipelined {
            return Err(Error::PipelinedRendering);
        }
        // SAFETY: No type of wgpu proves that the submissions to a queue are serialized, so `shared_device` needs that
        // promise from the caller and has no safe form. The check above confirms that pipelined rendering is off. The
        // contract of `SceneProducerPlugin::new` serializes the submissions of Bevy with the submissions of the export.
        let device = unsafe { shared_device(world) }?;
        world.insert_resource(Exports::new(device));
        Ok(())
    }
}

fn render_ready(selection: Res<Selection>, bridge: Res<SceneBridge>) -> bool {
    selection.into_inner().selected.is_some() && bridge.into_inner().stats().pending == 0
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
    frames_in_flight: Res<FramesInFlight>,
    bridge: Res<SceneBridge>,
    mut targets: ResMut<Targets>,
    mut selection: ResMut<Selection>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Camera, With<SceneCamera>>,
) -> Result {
    let (size, FramesInFlight(frames_in_flight), bridge) = (
        *size.into_inner(),
        *frames_in_flight.into_inner(),
        bridge.into_inner(),
    );
    selection.selected = None;
    if cameras.is_empty() {
        return Ok(());
    }
    // While an export is pending, the ring selects no target. The targets leased by the consumer are left alone, and
    // no render is queued behind incomplete scene work.
    if bridge.stats().pending > 0 {
        // The cameras remain active, so the extracted views and their HDR and depth caches are not
        // dropped. The Render set runs only while `render_ready` is true, so the graph does not render
        // into the leased camera target.
        bridge.busy();
        return Ok(());
    }
    if (targets.retired.iter()).all(|t| t.state.get() == SlotState::Free) {
        // Dropping the last handles of the retired targets frees their images.
        targets.retired.clear();
    }
    let resized = targets.current.iter().any(|t| t.size != size);
    if resized && targets.retired.is_empty() {
        targets.retired = std::mem::take(&mut targets.current);
    }
    if targets.current.is_empty() {
        // The consumer reads at most `frames_in_flight` scenes. The ring has one more target for the next render.
        targets.current = (0..=frames_in_flight.get())
            .map(|_| new_target(&mut images, size))
            .collect();
    }
    let target = (targets.current.iter())
        .find(|t| t.state.get() == SlotState::Free)
        .cloned()
        .ok_or(Error::ConsumerHoldsEveryScene)?;
    target.state.set(SlotState::Rendering);
    for mut camera in &mut cameras {
        camera.is_active = true;
        camera.target = RenderTarget::Image(target.image.clone().into());
    }
    selection.selected = Some(Selected {
        target,
        generation: bridge.generation(),
        at: Instant::now(),
    });
    Ok(())
}

/// Publishes the scenes whose releases are complete, without waiting for pending releases. Call it while a worker
/// thread stalls the simulation or the render of the app.
///
/// # Errors
/// Fails when the export cannot read the state of its releases or export a completed scene.
pub fn poll_app(app: &mut App) -> Result<(), Error> {
    with_exports(app, export::collect)
}

/// Waits for every pending release of the producer and publishes its scene, for teardown only. Before the call, the
/// consumer must have dropped every lease after waiting for the GPU fence of its reads.
///
/// # Errors
/// Fails when the export cannot wait for its releases or export a completed scene.
pub fn drain_app(app: &mut App) -> Result<(), Error> {
    with_exports(app, export::drain)
}

/// Runs `work` on the export state of the render world, which exists once the plugin has installed it.
fn with_exports(
    app: &mut App,
    work: fn(&SceneBridge, &mut Exports) -> Result<(), Error>,
) -> Result<(), Error> {
    let world = (app.get_sub_app_mut(RenderApp))
        .ok_or(Error::ExportsMissing)?
        .world_mut();
    let bridge = (world.get_resource::<SceneBridge>().cloned()).ok_or(Error::ExportsMissing)?;
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

    fn world(
        size: SceneSize,
        frames_in_flight: usize,
        bridge: &SceneBridge,
        targets: Targets,
    ) -> Result<(World, Entity), TestError> {
        let frames_in_flight =
            NonZeroUsize::new(frames_in_flight).ok_or(TestError::ZeroFramesInFlight)?;
        let mut world = World::new();
        world.insert_resource(size);
        world.insert_resource(FramesInFlight(frames_in_flight));
        world.insert_resource(bridge.clone());
        world.insert_resource(targets);
        world.insert_resource(Selection::default());
        world.insert_resource(Assets::<Image>::default());
        let camera = world.spawn((Camera::default(), SceneCamera)).id();
        Ok((world, camera))
    }

    /// Rotates the export targets of `world` once.
    fn rotate_targets(world: &mut World) -> Result<(), TestError> {
        world
            .run_system_cached::<Result, _, _>(rotate)?
            .map_err(TestError::Rotation)
    }

    fn target(size: SceneSize, state: SlotState) -> Target {
        let target = Target {
            image: Handle::default(),
            size,
            state: TargetState::new(),
        };
        target.state.set(state);
        target
    }

    #[test]
    fn incomplete_scene_keeps_preparation_active_but_cannot_render() -> Result<(), TestError> {
        let bridge = SceneBridge::default();
        bridge.submit();
        let size = scene_size()?;
        let free = target(size, SlotState::Free);
        let targets = Targets {
            current: vec![free.clone()],
            retired: Vec::new(),
        };
        let (mut world, camera) = world(size, 1, &bridge, targets)?;
        rotate_targets(&mut world)?;
        assert!(
            world
                .get::<Camera>(camera)
                .ok_or(TestError::NoCamera)?
                .is_active
        );
        assert!(world.resource::<Selection>().selected.is_none());
        assert!(!world.run_system_cached(render_ready)?);
        assert_eq!(free.state.get(), SlotState::Free);
        assert_eq!(bridge.stats().pending, 1);
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
    fn the_ring_has_one_target_more_than_the_frames_in_flight() -> Result<(), TestError> {
        let (mut world, _) = world(
            scene_size()?,
            2,
            &SceneBridge::default(),
            Targets::default(),
        )?;
        rotate_targets(&mut world)?;
        let targets = world.resource::<Targets>();
        assert_eq!(targets.current.len(), 3);
        let selected =
            (world.resource::<Selection>().selected.clone()).ok_or(TestError::NoSelection)?;
        assert_eq!(selected.target.state.get(), SlotState::Rendering);
        assert!(
            targets
                .current
                .iter()
                .any(|t| t.image == selected.target.image)
        );
        Ok(())
    }

    #[test]
    fn a_consumer_that_holds_every_target_is_an_error() -> Result<(), TestError> {
        let size = scene_size()?;
        let targets = Targets {
            current: vec![target(size, SlotState::Exported)],
            retired: Vec::new(),
        };
        let (mut world, _) = world(size, 1, &SceneBridge::default(), targets)?;
        assert!(world.run_system_cached::<Result, _, _>(rotate)?.is_err());
        assert!(world.resource::<Selection>().selected.is_none());
        Ok(())
    }
}
