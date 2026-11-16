// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::layer::layer_client::LayerClient;
use crate::app::wire_messages::WireMessage;
use crate::sctk;
use crate::test_error::TestError;
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

const FIRST: SurfaceId = SurfaceId::numbered(1);
const SECOND: SurfaceId = SurfaceId::numbered(2);

fn destroys_layer(sent: &[WireMessage], role: u32) -> bool {
    let destroy = WireMessage::new(role, zwlr_layer_surface_v1::REQ_DESTROY_OPCODE, &[]);
    sent.contains(&destroy)
}

#[test]
fn a_retired_surface_is_not_live_and_remains_until_its_release() -> Result<(), TestError> {
    let mut client = LayerClient::new(4)?;
    let (surface, role) = client.native_surface()?;
    let mut surfaces = NativeSurfaces::default();
    surfaces.insert(FIRST, surface);
    assert!(surfaces.is_live(FIRST));
    assert_eq!(surfaces.find_live(|_| true), Some(FIRST));
    surfaces.retire(FIRST);
    assert!(!surfaces.is_live(FIRST));
    assert!(surfaces.live(FIRST).is_none());
    assert_eq!(surfaces.find_live(|_| true), None);
    surfaces.retire(FIRST);
    assert!(!destroys_layer(&client.flush()?, role));
    drop(surfaces.release(FIRST));
    assert!(destroys_layer(&client.flush()?, role));
    assert!(surfaces.release(FIRST).is_none());
    Ok(())
}

#[test]
fn a_live_surface_is_not_released() -> Result<(), TestError> {
    let mut client = LayerClient::new(4)?;
    let (surface, role) = client.native_surface()?;
    let mut surfaces = NativeSurfaces::default();
    surfaces.insert(FIRST, surface);
    assert!(surfaces.release(FIRST).is_none());
    assert!(surfaces.is_live(FIRST));
    assert!(!destroys_layer(&client.flush()?, role));
    Ok(())
}

#[test]
fn clearing_destroys_the_live_and_the_retired_surfaces() -> Result<(), TestError> {
    let mut live_client = LayerClient::new(4)?;
    let (live, live_role) = live_client.native_surface()?;
    let mut retired_client = LayerClient::new(4)?;
    let (retired, retired_role) = retired_client.native_surface()?;
    let mut surfaces = NativeSurfaces::default();
    surfaces.insert(FIRST, live);
    surfaces.insert(SECOND, retired);
    surfaces.retire(SECOND);
    surfaces.clear();
    assert!(destroys_layer(&live_client.flush()?, live_role));
    assert!(destroys_layer(&retired_client.flush()?, retired_role));
    Ok(())
}
