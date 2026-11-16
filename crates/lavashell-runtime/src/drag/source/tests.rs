// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::pipe_transfers;

fn progress() -> DragProgress {
    DragProgress {
        pending: 1,
        sent: 0,
        failure: None,
        finished: false,
        action: Some(DragAction::Move),
        done: Box::new(|_, _| {}),
    }
}

#[test]
fn physical_drop_and_finish_cannot_complete_move_before_transmission() {
    let mut progress = progress();
    assert!(progress.take_result().is_none());
    progress.finished = true;
    assert!(progress.take_result().is_none());
    progress.pending = 0;
    assert!(matches!(
        progress.take_result(),
        Some(Err(Error::WithoutTransfer))
    ));
    progress.sent = 1;
    assert!(matches!(progress.take_result(), Some(Ok(DragAction::Move))));
}

#[test]
fn successful_send_requires_finish_and_failed_send_prevents_move() {
    let mut progress = progress();
    progress.pending = 0;
    progress.sent = 1;
    assert!(progress.take_result().is_none());
    progress.finished = true;
    let broken = std::io::ErrorKind::BrokenPipe.into();
    progress.failure = Some(Error::Transfer(pipe_transfers::Error::WritePipe(broken)));
    assert!(matches!(
        progress.take_result(),
        Some(Err(Error::Transfer(pipe_transfers::Error::WritePipe(_))))
    ));
    progress.action = None;
    assert!(matches!(
        progress.take_result(),
        Some(Err(Error::WithoutTransfer))
    ));
}
