// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use focus_wire::Wire;
use lavashell_core::prelude::{Density, DpExt, FrameTime, Rect, Size, Viewport};
use lavashell_renderer::{FrameOutcome, PresentFrame, SurfaceControl};

type Messages = Vec<(u32, u16, Vec<u32>)>;

fn present(wire: &mut Wire, frame: &PresentFrame) -> Result<Messages, TestError> {
    let control = wire.control.as_mut().ok_or(TestError::NoLayer)?;
    control.prepare(frame)?;
    control.before_present(frame);
    control.after_present(FrameOutcome::Shown);
    wire.flush()
}

#[test]
fn private_wire_sends_the_surface_state_of_a_frame_before_its_frame_request_and_only_once()
-> Result<(), TestError> {
    let mut wire = Wire::new(4)?;
    let layer = wire.layer(&LayerSurface::new("state"))?;
    assert!(!wire.flush()?.is_empty());
    let control = wire.control.as_mut().ok_or(TestError::NoLayer)?;
    let token = (control.layer_mut().ok_or(TestError::NoLayer)?).receive_configure(7);
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Configure(token));
    let region = vec![Rect::new(0.dp(), 0.dp(), 4.dp(), 2.dp())];
    state.apply(SurfaceChange::InputRegion(Some(region)));
    let frame = PresentFrame {
        time: FrameTime::default(),
        viewport: Viewport::new(Size::new(4.dp(), 2.dp()), Density::ONE),
        present_at: None,
        state: &state,
    };
    let surface = wire.surface.id().protocol_id();
    let role = layer.proxy.id().protocol_id();
    let sent = present(&mut wire, &frame)?;
    assert!(sent.contains(&(role, 6, vec![7])));
    let order: Vec<_> = (sent.iter())
        .map(|&(id, opcode, _)| (id, opcode))
        .filter(|message| [(surface, 5), (role, 6), (surface, 3)].contains(message))
        .collect();
    assert_eq!(order, vec![(surface, 5), (role, 6), (surface, 3)]);
    let again: Vec<_> = (present(&mut wire, &frame)?.into_iter())
        .map(|(id, opcode, _)| (id, opcode))
        .collect();
    assert_eq!(again, vec![(surface, 8), (surface, 3)]);
    let control = wire.control.as_mut().ok_or(TestError::NoLayer)?;
    control.commit_state(&state)?;
    assert_eq!(wire.flush()?, vec![(surface, 6, vec![])]);
    Ok(())
}
