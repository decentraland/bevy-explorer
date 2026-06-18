#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

use std::{future::Future, time::Duration};

use futures_timer::Delay;
use futures_util::{
    future::{select, Either},
    pin_mut, StreamExt,
};

/// Returned by [`with_timeout`] when the timer fires before the future resolves.
#[derive(Debug, Clone, Copy)]
pub struct Elapsed;

/// Race `fut` against a `dur` timer, returning `Err(Elapsed)` if the timer wins.
///
/// Cross-target replacement for `tokio::time::timeout` (which isn't available here
/// — tokio is built without the `time` feature, and it wouldn't work on wasm
/// anyway). `futures-timer` backs this with a native timer thread or wasm
/// `setTimeout`.
pub async fn with_timeout<F: Future>(dur: Duration, fut: F) -> Result<F::Output, Elapsed> {
    pin_mut!(fut);
    match select(fut, Delay::new(dur)).await {
        Either::Left((out, _)) => Ok(out),
        Either::Right(_) => Err(Elapsed),
    }
}

/// Failure modes for [`read_to_end_idle`].
pub enum IdleReadError {
    /// No body chunk arrived within the idle window — a stalled transfer.
    Idle,
    /// The underlying HTTP body stream errored.
    Http(reqwest::Error),
}

/// Read a response body to completion, failing with [`IdleReadError::Idle`] if no
/// chunk arrives within `idle`.
///
/// Unlike a total request timeout, the overall duration is unbounded as long as
/// data keeps flowing, so this won't kill a slow-but-progressing download — it
/// only trips on an actual stall. Dropping the returned future drops the stream,
/// which (on wasm) fires reqwest's `AbortGuard` to cancel the in-flight fetch.
pub async fn read_to_end_idle(
    response: reqwest::Response,
    idle: Duration,
) -> Result<Vec<u8>, IdleReadError> {
    let mut stream = Box::pin(response.bytes_stream());
    let mut buf = Vec::new();
    loop {
        match with_timeout(idle, stream.next()).await {
            Ok(Some(Ok(chunk))) => buf.extend_from_slice(&chunk),
            Ok(Some(Err(e))) => return Err(IdleReadError::Http(e)),
            Ok(None) => return Ok(buf),
            Err(Elapsed) => return Err(IdleReadError::Idle),
        }
    }
}
