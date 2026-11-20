// SPDX-License-Identifier: GPL-3.0-only
use super::test_error::LayerTestError;
use super::*;
use crate::native::wire_messages::WireMessage;
use crate::test_error::TestError;
use lavashell_core::prelude::{
    Density, DpExt, FrameTime, PixelScale, Rect, ScaledViewport, Size, Viewport,
};
use lavashell_renderer::{FrameOutcome, PresentFrame, SurfaceControl};
use layer_client::{LayerClient, top_bar};

const ACK_CONFIGURE: u16 = zwlr_layer_surface_v1::REQ_ACK_CONFIGURE_OPCODE;
const SET_INPUT_REGION: u16 = wl_surface::REQ_SET_INPUT_REGION_OPCODE;
const SET_BUFFER_SCALE: u16 = wl_surface::REQ_SET_BUFFER_SCALE_OPCODE;
const FRAME: u16 = wl_surface::REQ_FRAME_OPCODE;
const COMMIT: u16 = wl_surface::REQ_COMMIT_OPCODE;

fn present(client: &mut LayerClient, frame: &PresentFrame) -> Result<Vec<WireMessage>, TestError> {
    let control = client.control.as_mut().ok_or(LayerTestError::NoLayer)?;
    control.prepare(frame)?;
    control.before_present(frame);
    control.after_present(FrameOutcome::Shown);
    client.flush()
}

#[test]
fn sends_the_surface_state_of_a_frame_before_its_frame_request_and_only_once()
-> Result<(), TestError> {
    let mut client = LayerClient::new(4)?;
    let layer = client.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
    assert_ne!(client.flush()?, Vec::new());
    let control = client.control.as_mut().ok_or(LayerTestError::NoLayer)?;
    let token = ConfigureToken::default();
    control.receive(LayerConfigure {
        token,
        serial: ProtocolSerial::new(7),
    });
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Configure(token));
    let region = vec![Rect::new(0.dp(), 0.dp(), 4.dp(), 2.dp())];
    state.apply(SurfaceChange::InputRegion(Some(region)));
    let frame = PresentFrame {
        time: FrameTime::default(),
        scaled_viewport: ScaledViewport {
            viewport: Viewport::new(Size::new(4.dp(), 2.dp()), Density::ONE),
            scale: PixelScale::SQUARE,
        },
        present_at: None,
        state: &state,
    };
    let surface = client.surface.id().protocol_id();
    let role = layer.id().protocol_id();
    let sent = present(&mut client, &frame)?;
    assert!(sent.contains(&WireMessage::new(role, ACK_CONFIGURE, &[7])));
    let order: Vec<_> = (sent.iter())
        .map(|message| (message.object, message.opcode))
        .filter(|message| {
            [
                (surface, SET_INPUT_REGION),
                (role, ACK_CONFIGURE),
                (surface, FRAME),
            ]
            .contains(message)
        })
        .collect();
    assert_eq!(
        order,
        vec![
            (surface, SET_INPUT_REGION),
            (role, ACK_CONFIGURE),
            (surface, FRAME)
        ]
    );
    let again: Vec<_> = (present(&mut client, &frame)?.into_iter())
        .map(|message| (message.object, message.opcode))
        .collect();
    assert_eq!(again, vec![(surface, SET_BUFFER_SCALE), (surface, FRAME)]);
    let control = client.control.as_mut().ok_or(LayerTestError::NoLayer)?;
    control.commit_state(&state)?;
    assert_eq!(
        client.flush()?,
        vec![WireMessage::new(surface, COMMIT, &[])]
    );
    Ok(())
}

#[test]
fn acknowledges_a_layer_configure_with_the_frame_that_contains_its_token() -> Result<(), TestError>
{
    let mut client = LayerClient::new(4)?;
    let layer = client.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
    assert_ne!(client.flush()?, Vec::new());
    let (message, event) = translate_configure(ProtocolSerial::new(9), 0, 30);
    let configured = SurfaceEvent::Configured {
        width: None,
        height: Some(30.dp()),
        token: Some(message.token),
        window: None,
    };
    assert_eq!(event, configured);
    let control = client.control.as_mut().ok_or(LayerTestError::NoLayer)?;
    control.receive(message);
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Configure(message.token));
    let frame = PresentFrame {
        time: FrameTime::default(),
        scaled_viewport: ScaledViewport {
            viewport: Viewport::new(Size::new(4.dp(), 30.dp()), Density::ONE),
            scale: PixelScale::SQUARE,
        },
        present_at: None,
        state: &state,
    };
    let role = layer.id().protocol_id();
    assert!(present(&mut client, &frame)?.contains(&WireMessage::new(role, ACK_CONFIGURE, &[9])));
    Ok(())
}
