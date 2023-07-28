use ffmpeg_next::{
    format::{context::Input, input},
    Packet,
};

pub const BUFFER_TIME: f64 = 10.0;

pub trait PacketIter {
    fn is_eof(&self) -> bool;
    fn try_next(&mut self) -> Option<(usize, Packet)>;
    fn blocking_next(&mut self) -> Option<(usize, Packet)>;
    fn reset(&mut self);
}

// input stream wrapper allows reloading
pub struct InputWrapper {
    input: Input,
    path: String,
    is_eof: bool,
}

impl InputWrapper {
    pub fn new(input: Input, path: String) -> Self {
        Self {
            input,
            path,
            is_eof: false,
        }
    }
}

impl PacketIter for InputWrapper {
    fn is_eof(&self) -> bool {
        self.is_eof
    }

    fn try_next(&mut self) -> Option<(usize, Packet)> {
        let mut packet = Packet::empty();

        match packet.read(&mut self.input) {
            Ok(..) => Some((packet.stream(), packet)),
            Err(ffmpeg_next::util::error::Error::Eof) => {
                self.is_eof = true;
                None
            }
            _ => None,
        }
    }

    fn blocking_next(&mut self) -> Option<(usize, Packet)> {
        let mut packet = Packet::empty();

        loop {
            match packet.read(&mut self.input) {
                Ok(..) => return Some((packet.stream(), packet)),
                Err(ffmpeg_next::util::error::Error::Eof) => {
                    self.is_eof = true;
                    return None;
                }
                Err(..) => (),
            }
        }
    }

    fn reset(&mut self) {
        if self.input.seek(0, ..).is_err() {
            // reload
            if let Ok(reloaded_input) = input(&self.path) {
                self.input = reloaded_input;
            }
        }

        self.is_eof = false;
    }
}
