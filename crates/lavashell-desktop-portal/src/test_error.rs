// SPDX-License-Identifier: GPL-3.0-only
use crate::PortalError;
use lavashell_test_fixtures::test_bus::TestBusError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the bus failed")]
    Bus(#[from] zbus::Error),
    #[error("the portal client failed")]
    Portal(#[from] PortalError),
    #[error(transparent)]
    TestBus(#[from] TestBusError),
    #[error("the changes ended before the test read one")]
    ChangesEnded,
}
