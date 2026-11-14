// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    AccessibilityEvent, ClipboardEvent, DeviceEvent, DragSourceEvent, DragTargetEvent,
    FocusGrabEvent, LockEvent, OutputEvent, OutputId, PointerDevice, SettingsEvent, SurfaceEvent,
    WatchEvent, WatchId,
};
use lavashell_actor::{Effect, Merge};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::TransferId;
use lavashell_core::ui::TouchId;
use std::mem::{self, Discriminant};

/// What the host did, as a platform reports it to the runtime.
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
    Lock(LockEvent),
    FocusGrab(FocusGrabEvent),
    Accessibility(AccessibilityEvent),
}

impl From<(SurfaceId, SurfaceEvent)> for PlatformEvent {
    fn from((surface, event): (SurfaceId, SurfaceEvent)) -> Self {
        Self::Surface(surface, event)
    }
}

/// Wraps each event of one concern in the variant of `PlatformEvent` for that concern.
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
    LockEvent => Lock,
    FocusGrabEvent => FocusGrab,
    AccessibilityEvent => Accessibility,
}

/// A value that an event sets as a whole, so that a later event for the same value replaces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKey {
    PointerPosition(PointerDevice),
    Scroll(PointerDevice),
    TouchPosition(TouchId),
    Configuration(SurfaceId),
    Density(SurfaceId),
    Scale(SurfaceId),
    Setting(Discriminant<SettingsEvent>),
    OutputGeometry(OutputId),
    OutputRefresh(OutputId),
    Readiness(WatchId),
    DragPosition(TransferId),
}

impl Merge for PlatformEvent {
    type Key = EventKey;

    /// A pointer motion, a touch motion, a configure, a density, a scale, a setting, the geometry or the refresh
    /// interval of an output, a ready watch and the motion of a drag replace the earlier event for the same value. A
    /// configure replaces the waiting one because only the newest configure is acknowledged. Neighbouring scrolls of
    /// one pointer add up. A pointer motion and a scroll of one pointer fence each other, so neither passes the other.
    /// Every other event builds on the events before it, so the order of presses, releases and keys is kept.
    fn effect(&self) -> Effect<EventKey> {
        let key = match self {
            Self::Device(DeviceEvent::PointerMotion { pointer, .. }) => {
                return Effect::OverwritesAndFences {
                    key: EventKey::PointerPosition(*pointer),
                    fenced: EventKey::Scroll(*pointer),
                };
            }
            Self::Device(DeviceEvent::Axis { pointer, .. }) => {
                return Effect::OverwritesAndFences {
                    key: EventKey::Scroll(*pointer),
                    fenced: EventKey::PointerPosition(*pointer),
                };
            }
            Self::Device(DeviceEvent::TouchMotion(sample)) => EventKey::TouchPosition(sample.id),
            Self::Surface(surface, SurfaceEvent::Configured { .. }) => {
                EventKey::Configuration(*surface)
            }
            Self::Surface(surface, SurfaceEvent::DensityChanged(_)) => EventKey::Density(*surface),
            Self::Surface(surface, SurfaceEvent::ScaleChanged(_)) => EventKey::Scale(*surface),
            Self::Settings(setting) => EventKey::Setting(mem::discriminant(setting)),
            Self::Output(OutputEvent::Updated(output, _)) => EventKey::OutputGeometry(*output),
            Self::Output(OutputEvent::Refresh(output, _)) => EventKey::OutputRefresh(*output),
            Self::Watch(WatchEvent::Ready(watch)) => EventKey::Readiness(*watch),
            Self::DragTarget(DragTargetEvent::Motion(drag, _)) => EventKey::DragPosition(*drag),
            _ => return Effect::BuildsOn,
        };
        Effect::Overwrites(key)
    }

    /// A later scroll adds its delta to this scroll and sets the newest position. Every other later event replaces
    /// this event.
    fn absorb(&mut self, later: Self) {
        match (self, later) {
            (
                Self::Device(DeviceEvent::Axis { at, delta, .. }),
                Self::Device(DeviceEvent::Axis {
                    at: later_at,
                    delta: later_delta,
                    ..
                }),
            ) => {
                *at = later_at;
                *delta = delta.offset(later_delta.x, later_delta.y);
            }
            (earlier, later) => *earlier = later,
        }
    }
}

#[cfg(test)]
mod tests;
