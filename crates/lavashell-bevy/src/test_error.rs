// SPDX-License-Identifier: GPL-3.0-only

use crate::error::{Error, ShowFrameError};
use bevy::ecs::system::RegisteredSystemError;
use lavashell_actor::Closed;
use std::num::TryFromIntError;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the scene export failed")]
    Scene(#[from] Error),
    #[error("the scene pipeline of the overlay failed")]
    Pipeline(#[from] Arc<lavashell_runtime::scene_pipeline::Error>),
    #[error("the overlay could not show a frame")]
    ShowFrame(#[from] ShowFrameError),
    #[error("the choreographer of the overlay is gone")]
    Closed(#[from] Closed),
    #[error("a Vulkan call failed")]
    Vulkan(#[from] lavashell_vulkan::Error),
    #[error("the rotation of the export targets could not run")]
    RunRotation(#[from] RegisteredSystemError<(), ()>),
    #[error("the check whether a render is ready could not run")]
    RunRenderReady(#[from] RegisteredSystemError<(), bool>),
    #[error("a number does not fit its type")]
    Narrowing(#[from] TryFromIntError),
    #[error("a size of a scene is zero")]
    ZeroSize,
    #[error("the world has no scene camera")]
    NoCamera,
    #[error("the rotation selected no export target")]
    NoSelection,
    #[error("the instance has no physical device")]
    NoPhysicalDevice,
    #[error("the physical device has no graphics queue family")]
    NoGraphicsQueue,
}
