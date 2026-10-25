// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::Destination;
use lavashell_core::scene::DrawCommand;
use skia_safe::Canvas;

#[derive(Default)]
pub(crate) struct Scopes(Vec<Frame>);

struct Frame {
    canvas: usize,
    opaque: bool,
    saved: Vec<bool>,
}

impl Scopes {
    pub fn enter(&mut self, canvas: &Canvas, enabled: bool) -> bool {
        if !enabled {
            return false;
        }
        let identity = std::ptr::from_ref(canvas).addr();
        let opaque = self
            .0
            .last()
            .is_none_or(|parent| parent.canvas == identity && parent.opaque);
        self.0.push(Frame {
            canvas: identity,
            opaque,
            saved: Vec::new(),
        });
        true
    }

    pub fn leave(&mut self, entered: bool) {
        if entered {
            let _left_frame = self.0.pop();
        }
    }

    pub fn opaque(&self) -> bool {
        self.0.last().is_some_and(|frame| frame.opaque)
    }

    pub fn observe(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Save => self.open(true),
            DrawCommand::Opacity(alpha) | DrawCommand::OpacityLayer { alpha, .. } => {
                self.open(Destination::faded(*alpha) == Destination::Surface);
            }
            DrawCommand::EffectLayer { .. } => self.open(false),
            DrawCommand::Restore => self.close(),
            _ => {}
        }
    }

    pub fn open(&mut self, opaque: bool) {
        if let Some(frame) = self.0.last_mut() {
            frame.saved.push(frame.opaque);
            frame.opaque &= opaque;
        }
    }

    pub fn close(&mut self) {
        if let Some(frame) = self.0.last_mut()
            && let Some(opaque) = frame.saved.pop()
        {
            frame.opaque = opaque;
        }
    }
}
