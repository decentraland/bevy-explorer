use std::time::Duration;

use bevy::log::warn;
use common::util::ReportErr;
use ffmpeg_next::{Packet, format::context::Input};

pub const BUFFER_TIME: f64 = 10.0;

// consecutive read errors before a blocking read gives up and treats the input as ended
const MAX_READ_ERRORS: u32 = 100;
// cap on the backoff between failed blocking reads
const MAX_READ_BACKOFF: Duration = Duration::from_millis(100);

pub trait PacketIter {
    fn is_eof(&self) -> bool;
    fn try_next(&mut self) -> Option<(usize, Packet)>;
    fn blocking_next(&mut self) -> Option<(usize, Packet)>;
    fn reset(&mut self);
    fn seek_to(&mut self, time: f64);
}

// input stream wrapper allows reloading
pub struct InputWrapper {
    input: Option<Input>,
    pending_input: Option<tokio::sync::oneshot::Receiver<Input>>,
    path: String,
    is_eof: bool,
    read_errors: u32,
}

impl InputWrapper {
    pub fn new(input: Input, path: String) -> Self {
        Self {
            input: Some(input),
            pending_input: None,
            path,
            is_eof: false,
            read_errors: 0,
        }
    }
}

impl InputWrapper {
    fn get_input(&mut self, blocking: bool) -> Option<&mut Input> {
        if self.input.is_some() {
            #[allow(clippy::unnecessary_unwrap)] // required for borrow split
            return Some(self.input.as_mut().unwrap());
        }

        if blocking {
            if let Some(pending_input) = self.pending_input.take() {
                match pending_input.blocking_recv() {
                    Ok(input) => {
                        self.input = Some(input);
                        return Some(self.input.as_mut().unwrap());
                    }
                    Err(_) => {
                        self.is_eof = true;
                        return None;
                    }
                }
            }
        } else if let Some(pending_input) = self.pending_input.as_mut() {
            match pending_input.try_recv() {
                Ok(input) => {
                    self.input = Some(input);
                    return Some(self.input.as_mut().unwrap());
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.is_eof = true;
                    return None;
                }
            }
        }

        None
    }
}

impl PacketIter for InputWrapper {
    fn is_eof(&self) -> bool {
        self.is_eof
    }

    fn try_next(&mut self) -> Option<(usize, Packet)> {
        let input = self.get_input(false)?;
        let mut packet = Packet::empty();

        match packet.read(input) {
            Ok(..) => {
                self.read_errors = 0;
                Some((packet.stream(), packet))
            }
            Err(ffmpeg_next::util::error::Error::Eof) => {
                self.is_eof = true;
                None
            }
            _ => None,
        }
    }

    // returns None on a read error (after a short backoff) so the caller can check for disposal
    // before retrying. gives up and marks the input as ended after too many consecutive errors.
    fn blocking_next(&mut self) -> Option<(usize, Packet)> {
        let input = self.get_input(true)?;
        let mut packet = Packet::empty();

        match packet.read(input) {
            Ok(..) => {
                self.read_errors = 0;
                Some((packet.stream(), packet))
            }
            Err(ffmpeg_next::util::error::Error::Eof) => {
                self.is_eof = true;
                None
            }
            Err(e) => {
                self.read_errors += 1;
                if self.read_errors >= MAX_READ_ERRORS {
                    warn!(
                        "giving up on {} after repeated read errors: {e}",
                        crate::util::loggable_url(&self.path)
                    );
                    self.read_errors = 0;
                    self.is_eof = true;
                } else {
                    std::thread::sleep(
                        Duration::from_millis(1 << self.read_errors.min(7)).min(MAX_READ_BACKOFF),
                    );
                }
                None
            }
        }
    }

    fn reset(&mut self) {
        let Some(input) = self.get_input(false) else {
            return;
        };

        if input.seek(0, ..).is_err() {
            // reload
            let (sx, rx) = tokio::sync::oneshot::channel();
            let path = self.path.clone();
            std::thread::spawn(move || {
                if let Ok(input) = ffmpeg_next::format::input(&path) {
                    let _ = sx.send(input);
                }
            });
            self.input = None;
            self.pending_input = Some(rx);
        }

        self.is_eof = false;
    }

    fn seek_to(&mut self, time: f64) {
        let Some(input) = self.get_input(false) else {
            return;
        };

        if input.seek((time * 1000000.0) as i64, ..).is_err() {
            // reload
            let (sx, rx) = tokio::sync::oneshot::channel();
            let path = self.path.clone();
            std::thread::spawn(move || {
                if let Ok(mut input) = ffmpeg_next::format::input(&path) {
                    input.seek((time * 1000000.0) as i64, ..).report();
                    let _ = sx.send(input);
                }
            });
            self.input = None;
            self.pending_input = Some(rx);
        }

        self.is_eof = false;
    }
}
