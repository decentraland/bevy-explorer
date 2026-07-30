//! Wasm Pulse driver — the browser WebTransport counterpart to the native ENet driver.
//!
//! The browser has no ENet, so wasm speaks the server's WebTransport transport instead, behind the
//! identical byte boundary ([`PulseDriverChannels`]): reliable frames ride a length-framed
//! bidirectional QUIC stream, unreliable frames ride datagrams carrying a {channelId, seq} header.
//! Everything above the boundary (handshake, decode, foreign-player glue) is shared with native and
//! untouched — only this file differs. The framing here mirrors the server's `StreamFraming` /
//! `DatagramFraming` exactly, so the two ends agree on the wire.
//!
//! WebTransport is async and single-threaded here, so instead of native's dedicated OS thread this
//! runs a handful of `spawn_local` tasks on the JS event loop — one draining outbound frames, one
//! reading the reliable stream, one reading datagrams, one awaiting session close — coordinated by a
//! shared stop flag. The stop flag is flipped by driver teardown (handle drop), a closed session, or
//! the protocol layer dropping the link (which closes the outbound channel); any of them unwinds all
//! four tasks and closes the transport.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use bevy::log::warn;
use futures_util::future::{select, Either};
use gloo_timers::future::TimeoutFuture;
use js_sys::{Reflect, Uint8Array};
use tokio::sync::mpsc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    ReadableStreamDefaultReader, WebTransport, WebTransportBidirectionalStream, WebTransportHash,
    WebTransportOptions, WritableStreamDefaultWriter,
};

use super::framing::{
    frame_datagram, frame_stream, StreamAssembler, CHANNEL_SEQUENCED, CHANNEL_UNSEQUENCED,
};
use super::transport::{
    PulseDisconnect, PulseDriverChannels, PulseFrame, PulseReliability, PulseStatus,
    PulseTransportConfig,
};

/// How often the outbound pump wakes to re-check the stop flag when no frames are flowing, so a
/// teardown that only flips the flag (rather than closing the outbound channel) is still noticed.
const STOP_POLL_MS: u32 = 200;

pub(super) fn spawn(
    config: PulseTransportConfig,
    channels: PulseDriverChannels,
    stop: Arc<AtomicBool>,
) {
    spawn_local(async move {
        run(config, channels, stop).await;
    });
}

async fn run(config: PulseTransportConfig, channels: PulseDriverChannels, stop: Arc<AtomicBool>) {
    let PulseDriverChannels {
        outbound,
        inbound,
        status,
        presence,
    } = channels;

    let _ = status.try_send(PulseStatus::Connecting);

    // Browsers only accept an HTTP/3 (`https`) WebTransport URL; bracket a bare IPv6 literal for the
    // authority.
    let url = format!("https://{}:{}/", format_host(&config.host), config.port);
    let transport = match open(&url, config.cert_hash.as_deref()) {
        Ok(transport) => transport,
        Err(err) => return fail(&status, format!("webtransport create failed: {err:?}")),
    };

    // `ready` resolves once the QUIC session + CONNECT handshake complete; a rejection here is
    // never-established → Failed (retryable), the same class as native's DNS/connect failures.
    if let Err(err) = JsFuture::from(transport.ready()).await {
        return fail(&status, format!("webtransport connect failed: {err:?}"));
    }
    let _ = status.try_send(PulseStatus::Connected);

    // The reliable channel is one client-opened bidi stream, full-duplex and length-framed both ways
    // (the server writes server→client reliable messages back on the same stream via `SendStream`).
    let stream = match JsFuture::from(transport.create_bidirectional_stream()).await {
        Ok(stream) => stream.unchecked_into::<WebTransportBidirectionalStream>(),
        Err(err) => return fail(&status, format!("webtransport open stream failed: {err:?}")),
    };
    let stream_writer = match stream.writable().get_writer() {
        Ok(writer) => writer,
        Err(err) => {
            return fail(
                &status,
                format!("webtransport stream writer failed: {err:?}"),
            )
        }
    };
    let stream_reader = stream
        .readable()
        .get_reader()
        .unchecked_into::<ReadableStreamDefaultReader>();

    let datagrams = transport.datagrams();
    let datagram_writer = match datagrams.writable().get_writer() {
        Ok(writer) => writer,
        Err(err) => {
            return fail(
                &status,
                format!("webtransport datagram writer failed: {err:?}"),
            )
        }
    };
    let datagram_reader = datagrams
        .readable()
        .get_reader()
        .unchecked_into::<ReadableStreamDefaultReader>();

    let transport = Rc::new(transport);

    // Reader tasks reassemble/forward inbound, gated by presence exactly like native. Each closes the
    // session (→ the watcher reports Disconnected) when its stream ends or errors.
    spawn_local(read_stream(
        stream_reader,
        inbound.clone(),
        presence.clone(),
        stop.clone(),
        transport.clone(),
    ));
    spawn_local(read_datagrams(
        datagram_reader,
        inbound,
        presence,
        stop.clone(),
        transport.clone(),
    ));

    // The one place that reports Disconnected, mapping the session's application close code onto a
    // `PulseDisconnect` (mirrors native's ENet disconnect-data mapping).
    spawn_local(watch_closed(transport.clone(), status, stop.clone()));

    // This task becomes the outbound pump.
    pump_outbound(outbound, stream_writer, datagram_writer, stop, transport).await;
}

/// Build the `WebTransport`, pinning the server cert by SHA-256 when a hash is supplied (dev
/// self-signed certs); otherwise default CA trust (production).
fn open(url: &str, cert_hash: Option<&[u8]>) -> Result<WebTransport, JsValue> {
    let options = WebTransportOptions::new();
    if let Some(hash) = cert_hash {
        let entry = WebTransportHash::new();
        entry.set_algorithm("sha-256");
        let value = Uint8Array::new_with_length(hash.len() as u32);
        value.copy_from(hash);
        entry.set_value_u8_array(&value);
        options.set_server_certificate_hashes(&[entry]);
    }
    WebTransport::new_with_options(url, &options)
}

/// Drain outbound frames onto the QUIC session: reliable → the length-framed bidi stream, unreliable
/// → a {channelId, seq} datagram. Returns when the link closes, a send fails, or stop is set; then
/// closes the session so the watcher reports the disconnect.
async fn pump_outbound(
    mut outbound: mpsc::Receiver<PulseFrame>,
    stream_writer: WritableStreamDefaultWriter,
    datagram_writer: WritableStreamDefaultWriter,
    stop: Arc<AtomicBool>,
    transport: Rc<WebTransport>,
) {
    // Per-channel outbound sequence, mirroring the server's `DatagramSequencer` (wraps at u32).
    let mut seq_sequenced: u32 = 0;
    let mut seq_unsequenced: u32 = 0;

    while !stop.load(Ordering::Relaxed) {
        // Race the next frame against a short timeout so a bare stop flip (no channel close) is still
        // observed promptly.
        let recv = std::pin::pin!(outbound.recv());
        let timer = std::pin::pin!(TimeoutFuture::new(STOP_POLL_MS));
        let frame = match select(recv, timer).await {
            Either::Left((Some(frame), _)) => frame,
            // Outbound channel closed — the protocol layer dropped the link. Tear down.
            Either::Left((None, _)) => break,
            // Timeout — loop and re-check stop.
            Either::Right(_) => continue,
        };

        match frame.reliability {
            PulseReliability::Reliable => {
                // The reliable channel is a single ordered stream: a failed write means it's broken
                // and unrecoverable, so drop the session (the watcher then reports a retryable
                // disconnect and the protocol layer reconnects).
                let framed = frame_stream(&frame.bytes);
                if let Err(err) = write(&stream_writer, &framed).await {
                    warn!("pulse: webtransport reliable send failed: {err:?}");
                    break;
                }
            }
            PulseReliability::UnreliableSequenced | PulseReliability::UnreliableUnsequenced => {
                // Unreliable: a dropped datagram (send failure or over-MTU) is tolerable and expected
                // — like the reference client, skip this one and keep the session, rather than bounce
                // the whole connection (a full re-handshake + re-teleport) over one lost packet.
                let (channel, seq) = match frame.reliability {
                    PulseReliability::UnreliableSequenced => {
                        (CHANNEL_SEQUENCED, next_seq(&mut seq_sequenced))
                    }
                    _ => (CHANNEL_UNSEQUENCED, next_seq(&mut seq_unsequenced)),
                };
                let framed = frame_datagram(channel, seq, &frame.bytes);
                if let Err(err) = write(&datagram_writer, &framed).await {
                    warn!("pulse: webtransport datagram send failed (dropped): {err:?}");
                }
            }
        }
    }

    teardown(&stop, &transport);
}

/// Read the reliable bidi stream, reassemble length-framed messages, and forward each (while a
/// routing entity is alive) into the shared inbound channel.
async fn read_stream(
    reader: ReadableStreamDefaultReader,
    inbound: mpsc::Sender<Vec<u8>>,
    presence: Weak<()>,
    stop: Arc<AtomicBool>,
    transport: Rc<WebTransport>,
) {
    let mut assembler = StreamAssembler::default();

    while !stop.load(Ordering::Relaxed) {
        let chunk = match read_chunk(&reader).await {
            Ok(Some(chunk)) => chunk,
            // Stream done or errored — the session is going away.
            Ok(None) | Err(_) => break,
        };

        assembler.append(&chunk);
        loop {
            match assembler.next_message() {
                Ok(Some(message)) => surface(&inbound, &presence, message),
                Ok(None) => break,
                Err(err) => {
                    // Unrecoverable framing — the stream's next boundary is lost; drop the session.
                    warn!("pulse: {err}");
                    teardown(&stop, &transport);
                    return;
                }
            }
        }
    }

    teardown(&stop, &transport);
}

/// Read inbound datagrams. Server→client datagrams are bare `ServerMessage`s (no transport header —
/// each already carries its own body sequence), so each chunk is forwarded as-is.
async fn read_datagrams(
    reader: ReadableStreamDefaultReader,
    inbound: mpsc::Sender<Vec<u8>>,
    presence: Weak<()>,
    stop: Arc<AtomicBool>,
    transport: Rc<WebTransport>,
) {
    while !stop.load(Ordering::Relaxed) {
        match read_chunk(&reader).await {
            Ok(Some(chunk)) => surface(&inbound, &presence, chunk),
            Ok(None) | Err(_) => break,
        }
    }

    teardown(&stop, &transport);
}

/// Await session close and report the single Disconnected. `closed` resolves with the close info on a
/// clean close (either side) and rejects on a transport error; either way we were connected, so it's
/// a Disconnect (not a Failed). The application close code — set when the server calls its
/// `Disconnect(reason)` — maps onto a [`PulseDisconnect`]; a transport error carries none, so it
/// defaults to `None` (retryable).
async fn watch_closed(
    transport: Rc<WebTransport>,
    status: mpsc::Sender<PulseStatus>,
    stop: Arc<AtomicBool>,
) {
    let reason = match JsFuture::from(transport.closed()).await {
        Ok(info) => PulseDisconnect::from_code(info.get_close_code().unwrap_or(0)),
        Err(_) => PulseDisconnect::None,
    };
    let _ = status.try_send(PulseStatus::Disconnected(reason));
    stop.store(true, Ordering::Relaxed);
}

/// Forward one inbound message only while a routing entity is alive (we're on a Pulse realm) — the
/// same gate as native. Off-realm we keep draining the streams to keep them flowing but drop the
/// payload; the decoder's resulting gap is healed by resync/teleport on return.
fn surface(inbound: &mpsc::Sender<Vec<u8>>, presence: &Weak<()>, message: Vec<u8>) {
    if presence.strong_count() > 1 {
        let _ = inbound.try_send(message);
    }
}

/// Read one chunk from a reader: `Ok(Some(bytes))` for data, `Ok(None)` when the stream is done,
/// `Err` on a read error.
async fn read_chunk(reader: &ReadableStreamDefaultReader) -> Result<Option<Vec<u8>>, JsValue> {
    let result = JsFuture::from(reader.read()).await?;
    if Reflect::get(&result, &JsValue::from_str("done"))?
        .as_bool()
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let value = Reflect::get(&result, &JsValue::from_str("value"))?;
    Ok(Some(value.dyn_into::<Uint8Array>()?.to_vec()))
}

/// Write one framed message to a QUIC writer (reliable stream or datagram), awaiting it for
/// backpressure and ordering.
async fn write(writer: &WritableStreamDefaultWriter, framed: &[u8]) -> Result<(), JsValue> {
    let chunk = Uint8Array::new_with_length(framed.len() as u32);
    chunk.copy_from(framed);
    JsFuture::from(writer.write_with_chunk(&chunk)).await?;
    Ok(())
}

fn next_seq(seq: &mut u32) -> u32 {
    let current = *seq;
    *seq = seq.wrapping_add(1);
    current
}

/// Stop the other tasks and close the session; the watcher observes `closed` resolving and reports
/// the disconnect. Idempotent — repeated calls just re-set the flag and re-close.
fn teardown(stop: &AtomicBool, transport: &WebTransport) {
    stop.store(true, Ordering::Relaxed);
    transport.close();
}

fn fail(status: &mpsc::Sender<PulseStatus>, message: String) {
    warn!("pulse: {message}");
    let _ = status.try_send(PulseStatus::Failed(message));
}

fn format_host(host: &str) -> String {
    // Bracket a bare IPv6 literal for the URL authority; leave hostnames / IPv4 as-is.
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}
