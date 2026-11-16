// SPDX-License-Identifier: GPL-3.0-only
use crate::input::DeviceEventKey;
use crate::output::OutputEventKey;
use crate::surface::SurfaceEventKey;
use crate::{
    AccessibilityEvent, ClipboardEvent, DeviceEvent, DragSourceEvent, DragTargetEvent,
    FocusGrabEvent, OutputEvent, SessionLockEvent, SettingsEvent, SurfaceEvent, WatchEvent,
    WatchId,
};
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::TransferId;
use std::mem::Discriminant;

#[derive(Debug)]
pub enum PlatformEvent {
    Surface(SurfaceId, SurfaceEvent),
    Output(OutputEvent),
    Device(DeviceEvent),
    Settings(SettingsEvent),
    Watch(WatchEvent),
    Clipboard(ClipboardEvent),
    DragTarget(DragTargetEvent),
    DragSource(DragSourceEvent),
    Lock(SessionLockEvent),
    FocusGrab(FocusGrabEvent),
    Accessibility(AccessibilityEvent),
}

impl From<(SurfaceId, SurfaceEvent)> for PlatformEvent {
    fn from((surface, event): (SurfaceId, SurfaceEvent)) -> Self {
        Self::Surface(surface, event)
    }
}

macro_rules! from_concern_events {
    ($($event:ident => $variant:ident),* $(,)?) => {
        $(
            impl From<$event> for PlatformEvent {
                fn from(event: $event) -> Self {
                    Self::$variant(event)
                }
            }
        )*
    };
}

from_concern_events! {
    OutputEvent => Output,
    DeviceEvent => Device,
    SettingsEvent => Settings,
    WatchEvent => Watch,
    ClipboardEvent => Clipboard,
    DragTargetEvent => DragTarget,
    DragSourceEvent => DragSource,
    SessionLockEvent => Lock,
    FocusGrabEvent => FocusGrab,
    AccessibilityEvent => Accessibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKey {
    Surface(SurfaceId, SurfaceEventKey),
    Output(OutputEventKey),
    Device(DeviceEventKey),
    Setting(Discriminant<SettingsEvent>),
    Readiness(WatchId),
    DragPosition(TransferId),
}

impl Merge for PlatformEvent {
    type Key = EventKey;

    fn effect(&self) -> MergeEffect<EventKey> {
        match self {
            Self::Surface(surface, event) => {
                event.effect().map(|key| EventKey::Surface(*surface, key))
            }
            Self::Output(event) => event.effect().map(EventKey::Output),
            Self::Device(event) => event.effect().map(EventKey::Device),
            Self::Settings(event) => event.effect().map(EventKey::Setting),
            Self::Watch(event) => event.effect().map(EventKey::Readiness),
            Self::DragTarget(event) => event.effect().map(EventKey::DragPosition),
            Self::Clipboard(_)
            | Self::DragSource(_)
            | Self::Lock(_)
            | Self::FocusGrab(_)
            | Self::Accessibility(_) => MergeEffect::Appends,
        }
    }

    fn absorb(&mut self, later: Self) {
        match (self, later) {
            (Self::Device(earlier), Self::Device(later)) => earlier.absorb(later),
            (earlier, later) => *earlier = later,
        }
    }
}

#[cfg(test)]
mod tests;
