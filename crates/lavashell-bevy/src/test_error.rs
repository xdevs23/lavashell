// SPDX-License-Identifier: GPL-3.0-only
//! The failures that the tests of this crate return in place of a panic.

use crate::Error;
use bevy::ecs::error::BevyError;
use bevy::ecs::system::RegisteredSystemError;
use std::num::TryFromIntError;
use std::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the scene export failed")]
    Scene(#[from] Error),
    #[error("a Vulkan call failed")]
    Vulkan(#[from] lavashell_vulkan::Error),
    #[error("the rotation of the export targets could not run")]
    RunRotation(#[from] RegisteredSystemError<(), Result<(), BevyError>>),
    #[error("the rotation of the export targets failed: {0}")]
    Rotation(BevyError),
    #[error("the check whether a render is ready could not run")]
    RunRenderReady(#[from] RegisteredSystemError<(), bool>),
    #[error("no timing record arrived")]
    Receive(#[from] mpsc::RecvError),
    #[error("a number does not fit its type")]
    Narrowing(#[from] TryFromIntError),
    #[error("a size of a scene is zero")]
    ZeroSize,
    #[error("the frames in flight are zero")]
    ZeroFramesInFlight,
    #[error("the world has no scene camera")]
    NoCamera,
    #[error("the rotation selected no export target")]
    NoSelection,
    #[error("the instance has no physical device")]
    NoPhysicalDevice,
    #[error("the physical device has no graphics queue family")]
    NoGraphicsQueue,
}
