// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::Viewport;
use lavashell_core::output::OutputInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use std::time::Duration;

/// A physical key of the focused surface, as a Linux evdev code independent of the keyboard layout and of text
/// editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub surface: SurfaceId,
    pub code: u32,
    pub pressed: bool,
    pub repeat: bool,
}

/// A surface that gained or lost the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardFocus {
    pub surface: SurfaceId,
    pub focused: bool,
}

type ViewObserver = Box<dyn FnMut(SurfaceId, Viewport, &View)>;

/// The callbacks through which an app follows its run. Each one runs on the thread of the views.
#[derive(Default)]
pub struct Observers {
    /// Runs before the first compose of each render, with the time since the previous one.
    pub tick: Option<Box<dyn FnMut(Duration)>>,
    /// Runs before the first compose of each render, with the timing of its frame.
    pub frame: Option<Box<dyn FnMut(FrameTime)>>,
    /// Receives each report about a submitted, presented or discarded frame.
    pub frame_report: Option<Box<dyn FnMut(FrameReport)>>,
    /// Receives every physical key press and release before the view, and returns whether it consumes the key.
    pub key: Option<Box<dyn FnMut(HostKey) -> bool>>,
    pub keyboard_focus: Option<Box<dyn FnMut(KeyboardFocus)>>,
    /// Inspects each view after a frame of it was drawn.
    pub view: Option<ViewObserver>,
    /// Receives the output that shows a surface each time it changes.
    pub output: Option<Box<dyn FnMut(SurfaceId, OutputInfo)>>,
}
