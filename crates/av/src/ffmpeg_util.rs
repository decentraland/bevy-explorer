use ffmpeg_next::{format::context::Input, Packet};

pub const BUFFER_TIME: f64 = 10.0;

pub trait PacketIter {
    fn is_eof(&self) -> bool;
    fn try_next(&mut self) -> Option<(usize, Packet)>;
    fn blocking_next(&mut self) -> Option<(usize, Packet)>;
    fn reset(&mut self);
}

// input stream wrapper allows reloading
pub struct InputWrapper {
    input: Option<Input>,
    pending_input: Option<tokio::sync::oneshot::Receiver<Input>>,
    path: String,
    is_eof: bool,
}

impl InputWrapper {
    pub fn new(input: Input, path: String) -> Self {
        Self {
            input: Some(input),
            pending_input: None,
            path,
            is_eof: false,
        }
    }
}

impl InputWrapper {
    fn get_input(&mut self, blocking: bool) -> Option<&mut Input> {
        if self.input.is_some() {
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
            Ok(..) => Some((packet.stream(), packet)),
            Err(ffmpeg_next::util::error::Error::Eof) => {
                self.is_eof = true;
                None
            }
            _ => None,
        }
    }

    fn blocking_next(&mut self) -> Option<(usize, Packet)> {
        let input = self.get_input(true)?;
        let mut packet = Packet::empty();

        loop {
            match packet.read(input) {
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
}
