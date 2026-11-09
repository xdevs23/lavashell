// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::io;

#[derive(Debug, thiserror::Error)]
enum CallerError {
    #[error(transparent)]
    Transfer(#[from] Error),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}

#[test]
fn a_transfer_started_after_the_watches_closed_is_cancelled() -> io::Result<()> {
    let watches = Watches::default();
    watches.close();
    let pipes = PipeTransfers::new(&watches);
    let outcome = Rc::new(RefCell::new(None));
    let reported = Rc::clone(&outcome);
    let done: ReadCompletion<CallerError> =
        Box::new(move |result| *reported.borrow_mut() = Some(result));
    let (reader, _writer) = io::pipe()?;
    let format = FormatId("text/plain".to_owned());
    let transfer = Transfer::receive(reader.into(), SurfaceId(1), format, done);
    assert!(pipes.start(transfer).is_none());
    assert!(matches!(
        *outcome.borrow(),
        Some(Err(CallerError::Cancelled(Cancellation::AppClosed)))
    ));
    Ok(())
}
