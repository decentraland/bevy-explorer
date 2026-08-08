#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

use std::{future::Future, pin::Pin, time::Duration};

use futures_timer::Delay;
use futures_util::{
    future::{select, Either},
    pin_mut, Stream, StreamExt,
};

/// Marker: the [`with_timeout`] timer fired before the future resolved.
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

impl FetchError<std::convert::Infallible> {
    /// Re-type a body-phase error (which can never be `Send`) to any send error type.
    pub fn widen<E>(self) -> FetchError<E> {
        match self {
            FetchError::Headers => FetchError::Headers,
            FetchError::Send(never) => match never {},
            FetchError::Status(status) => FetchError::Status(status),
            FetchError::Stalled => FetchError::Stalled,
            FetchError::Body(e) => FetchError::Body(e),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type BytesStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>;
#[cfg(target_arch = "wasm32")]
type BytesStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>>>>;

/// An in-flight response body being read incrementally.
///
/// Dropping a FetchStream aborts the transfer: reqwest's wasm AbortGuard lives
/// exactly as long as the body stream, and native closes the connection.
pub struct FetchStream {
    pub headers: reqwest::header::HeaderMap,
    stream: BytesStream,
    idle_timeout: Duration,
}

/// Like [`fetch`], but hand back the body stream after the headers phase so the
/// caller can consume chunks as they arrive (and abort by dropping).
pub async fn fetch_stream<E>(
    send: impl Future<Output = Result<reqwest::Response, E>>,
    headers_timeout: Duration,
    idle_timeout: Duration,
) -> Result<FetchStream, FetchError<E>> {
    let response = match with_timeout(headers_timeout, send).await {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => return Err(FetchError::Send(e)),
        Err(Elapsed) => return Err(FetchError::Headers),
    };

    if !response.status().is_success() {
        return Err(FetchError::Status(response.status()));
    }

    Ok(FetchStream {
        headers: response.headers().clone(),
        stream: Box::pin(response.bytes_stream()),
        idle_timeout,
    })
}

impl FetchStream {
    /// Ok(Some(chunk)), Ok(None) at clean end, Err(Stalled) after idle_timeout
    /// without a chunk, Err(Body) on transport error.
    pub async fn next_chunk(
        &mut self,
    ) -> Result<Option<bytes::Bytes>, FetchError<std::convert::Infallible>> {
        match with_timeout(self.idle_timeout, self.stream.next()).await {
            Ok(Some(Ok(chunk))) => Ok(Some(chunk)),
            Ok(Some(Err(e))) => Err(FetchError::Body(e)),
            Ok(None) => Ok(None),
            Err(Elapsed) => Err(FetchError::Stalled),
        }
    }
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
    let mut stream = fetch_stream(send, headers_timeout, idle_timeout).await?;
    let mut body = Vec::new();
    loop {
        match stream.next_chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            Ok(None) => {
                return Ok(FetchedResponse {
                    headers: stream.headers,
                    body,
                })
            }
            Err(e) => return Err(e.widen()),
        }
    }
}
