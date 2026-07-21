//! WebTransport wire framing, shared with the server (`StreamFraming` / `DatagramFraming` in
//! `decentraland/Pulse`). Split out from the wasm driver so it's unit-testable on the native
//! `cargo test` target — the driver itself is wasm-only, but this framing is pure byte logic. Reliable
//! messages are length-prefixed on the QUIC stream; unreliable ones carry a {channelId, seq} datagram
//! header.

/// Datagram channel ids — must match `WebTransportHostedService`/`WebTransportBotTransport` on the
/// server: 1 = unreliable-sequenced (server stale-drops by sequence), 2 = unreliable-unsequenced.
pub(super) const CHANNEL_SEQUENCED: u8 = 1;
pub(super) const CHANNEL_UNSEQUENCED: u8 = 2;

/// Length-prefix header on the reliable stream: a 4-byte big-endian payload length.
const STREAM_HEADER_SIZE: usize = 4;

/// Datagram header: 1-byte channel id + 4-byte big-endian sequence.
const DATAGRAM_HEADER_SIZE: usize = 5;

/// Cap on a single reassembled reliable message, matching the server's `WebTransport:MaxMessageBytes`.
/// A stream frame declaring more than this is treated as unrecoverable framing corruption.
const MAX_MESSAGE_BYTES: usize = 4096;

/// Length-prefix a reliable payload with its 4-byte big-endian length (server `StreamFraming`).
pub(super) fn frame_stream(payload: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(STREAM_HEADER_SIZE + payload.len());
    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    framed.extend_from_slice(payload);
    framed
}

/// Prefix an unreliable payload with its {channelId, seq} header (server `DatagramFraming`).
pub(super) fn frame_datagram(channel: u8, seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(DATAGRAM_HEADER_SIZE + payload.len());
    framed.push(channel);
    framed.extend_from_slice(&seq.to_be_bytes());
    framed.extend_from_slice(payload);
    framed
}

/// Reassembles length-prefixed reliable messages from raw stream chunks that split or coalesce frame
/// boundaries arbitrarily — the client mirror of the server's `StreamFrameReader`.
#[derive(Default)]
pub(super) struct StreamAssembler {
    buffer: Vec<u8>,
}

impl StreamAssembler {
    pub(super) fn append(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// Dequeue the next complete message (length prefix stripped); `Ok(None)` if a full frame is not
    /// buffered yet, `Err` if a frame declares a length past the cap (unrecoverable — the buffer is
    /// cleared so a stale header can't re-throw every chunk).
    pub(super) fn next_message(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.buffer.len() < STREAM_HEADER_SIZE {
            return Ok(None);
        }
        let length =
            u32::from_be_bytes(self.buffer[..STREAM_HEADER_SIZE].try_into().unwrap()) as usize;
        if length > MAX_MESSAGE_BYTES {
            self.buffer.clear();
            return Err(format!(
                "webtransport stream frame length {length} exceeds cap {MAX_MESSAGE_BYTES}"
            ));
        }
        let total = STREAM_HEADER_SIZE + length;
        if self.buffer.len() < total {
            return Ok(None);
        }
        let message = self.buffer[STREAM_HEADER_SIZE..total].to_vec();
        self.buffer.drain(..total);
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(assembler: &mut StreamAssembler) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(message) = assembler.next_message().unwrap() {
            out.push(message);
        }
        out
    }

    #[test]
    fn round_trips_a_single_frame() {
        let payload = b"hello pulse".to_vec();
        let mut assembler = StreamAssembler::default();
        assembler.append(&frame_stream(&payload));
        assert_eq!(drain(&mut assembler), vec![payload]);
    }

    #[test]
    fn empty_buffer_yields_nothing() {
        let mut assembler = StreamAssembler::default();
        assert_eq!(assembler.next_message().unwrap(), None);
    }

    #[test]
    fn reassembles_a_frame_split_at_every_byte_boundary() {
        let payload = b"a fragmented reliable message".to_vec();
        let framed = frame_stream(&payload);

        // A QUIC stream can deliver a frame split at an arbitrary point — including mid-header. Feeding
        // one byte at a time exercises every split, yielding the message only once the last byte lands.
        for split in 1..framed.len() {
            let mut assembler = StreamAssembler::default();
            assembler.append(&framed[..split]);
            assert_eq!(assembler.next_message().unwrap(), None, "split at {split}");
            assembler.append(&framed[split..]);
            assert_eq!(assembler.next_message().unwrap(), Some(payload.clone()));
            assert_eq!(assembler.next_message().unwrap(), None);
        }
    }

    #[test]
    fn splits_multiple_coalesced_frames_from_one_chunk() {
        // Several frames can arrive in a single read; each must be dequeued in order.
        let a = b"first".to_vec();
        let b = b"second, longer message".to_vec();
        let c = b"third".to_vec();
        let mut chunk = frame_stream(&a);
        chunk.extend_from_slice(&frame_stream(&b));
        chunk.extend_from_slice(&frame_stream(&c));

        let mut assembler = StreamAssembler::default();
        assembler.append(&chunk);
        assert_eq!(drain(&mut assembler), vec![a, b, c]);
    }

    #[test]
    fn handles_an_empty_payload_frame() {
        let mut assembler = StreamAssembler::default();
        assembler.append(&frame_stream(&[]));
        assert_eq!(drain(&mut assembler), vec![Vec::<u8>::new()]);
    }

    #[test]
    fn rejects_and_recovers_from_an_oversized_frame() {
        // A header declaring more than the cap is unrecoverable — the buffer is cleared so a valid
        // frame appended afterwards parses cleanly rather than re-hitting the stale header.
        let mut assembler = StreamAssembler::default();
        assembler.append(&(MAX_MESSAGE_BYTES as u32 + 1).to_be_bytes());
        assert!(assembler.next_message().is_err());

        let payload = b"recovered".to_vec();
        assembler.append(&frame_stream(&payload));
        assert_eq!(drain(&mut assembler), vec![payload]);
    }

    #[test]
    fn accepts_a_frame_exactly_at_the_cap() {
        let payload = vec![0x7u8; MAX_MESSAGE_BYTES];
        let mut assembler = StreamAssembler::default();
        assembler.append(&frame_stream(&payload));
        assert_eq!(drain(&mut assembler), vec![payload]);
    }

    #[test]
    fn frames_a_datagram_header() {
        let payload = b"movement".to_vec();
        let framed = frame_datagram(CHANNEL_SEQUENCED, 0x01020304, &payload);
        assert_eq!(framed[0], CHANNEL_SEQUENCED);
        assert_eq!(&framed[1..5], &[0x01, 0x02, 0x03, 0x04]); // big-endian sequence
        assert_eq!(&framed[5..], &payload[..]);
        assert_ne!(CHANNEL_SEQUENCED, CHANNEL_UNSEQUENCED);
    }
}
