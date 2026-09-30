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

/// Internal marker: the [`with_timeout`] timer fired before the future resolved.
struct Elapsed;

/// Race `fut` against a `dur` timer, returning `Err(Elapsed)` if the timer wins.
///
/// Cross-target replacement for `tokio::time::timeout` (which isn't available here
/// — tokio is built without the `time` feature, and it wouldn't work on wasm
/// anyway). `futures-timer` backs this with a native timer thread or wasm
/// `setTimeout`.
async fn with_timeout<F: Future>(dur: Duration, fut: F) -> Result<F::Output, Elapsed> {
    pin_mut!(fut);
    match select(fut, Delay::new(dur)).await {
        Either::Left((out, _)) => Ok(out),
        Either::Right(_) => Err(Elapsed),
    }
}

/// A successfully-fetched response: its headers and fully-read body.
pub struct FetchedResponse {
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

/// Why a [`fetch`] failed. `E` is the error type of the supplied send future —
/// `reqwest::Error` for a bare `RequestBuilder::send`, or e.g. `anyhow::Error` when
/// the send is routed through a wrapper.
pub enum FetchError<E> {
    /// Timed out awaiting the response headers (connect + time-to-first-byte).
    Headers,
    /// The send itself failed before any response was obtained.
    Send(E),
    /// The server returned a non-success status; the body is not read.
    Status(reqwest::StatusCode),
    /// The body transfer stalled — no chunk arrived within the idle window.
    Stalled,
    /// The body stream errored mid-transfer.
    Body(reqwest::Error),
}

/// Drive an HTTP request with two independent timeouts, returning the response
/// headers and fully-read body.
///
/// `headers_timeout` bounds the connect + headers phase (the only hang-guard on
/// wasm, which has no `connect_timeout`); `idle_timeout` bounds the gap between body
/// chunks. The *total* transfer time is deliberately unbounded, so a slow-but-
/// progressing download is never killed — only a genuine stall trips. A non-success
/// status short-circuits with [`FetchError::Status`] before the body is read.
/// Dropping the returned future cancels the in-flight request.
pub async fn fetch<E>(
    send: impl Future<Output = Result<reqwest::Response, E>>,
    headers_timeout: Duration,
    idle_timeout: Duration,
) -> Result<FetchedResponse, FetchError<E>> {
    let response = match with_timeout(headers_timeout, send).await {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => return Err(FetchError::Send(e)),
        Err(Elapsed) => return Err(FetchError::Headers),
    };

    if !response.status().is_success() {
        return Err(FetchError::Status(response.status()));
    }

    let headers = response.headers().clone();
    let mut stream = Box::pin(response.bytes_stream());
    let mut body = Vec::new();
    loop {
        match with_timeout(idle_timeout, stream.next()).await {
            Ok(Some(Ok(chunk))) => body.extend_from_slice(&chunk),
            Ok(Some(Err(e))) => return Err(FetchError::Body(e)),
            Ok(None) => return Ok(FetchedResponse { headers, body }),
            Err(Elapsed) => return Err(FetchError::Stalled),
        }
    }
}
