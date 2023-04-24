// buffer format helpers
#[derive(Debug)]
pub enum DclReaderError {
    Eof,
    ProtobufErr(prost::DecodeError),
}

impl From<prost::DecodeError> for DclReaderError {
    fn from(value: prost::DecodeError) -> Self {
        Self::ProtobufErr(value)
    }
}

pub struct DclReader<'a> {
    pos: usize,
    buffer: &'a [u8],
}

impl<'a> DclReader<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self { pos: 0, buffer }
    }

    pub fn read_u8(&mut self) -> Result<u8, DclReaderError> {
        Ok(u8::from_le_bytes(
            self.take_slice(1).try_into().or(Err(DclReaderError::Eof))?,
        ))
    }

    pub fn read_u16(&mut self) -> Result<u16, DclReaderError> {
        Ok(u16::from_le_bytes(
            self.take_slice(2).try_into().or(Err(DclReaderError::Eof))?,
        ))
    }

    pub fn read_u32(&mut self) -> Result<u32, DclReaderError> {
        Ok(u32::from_le_bytes(
            self.take_slice(4).try_into().or(Err(DclReaderError::Eof))?,
        ))
    }

    pub fn read_float(&mut self) -> Result<f32, DclReaderError> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    pub fn read_float3(&mut self) -> Result<[f32; 3], DclReaderError> {
        Ok([self.read_float()?, self.read_float()?, self.read_float()?])
    }

    pub fn read_float4(&mut self) -> Result<[f32; 4], DclReaderError> {
        Ok([
            self.read_float()?,
            self.read_float()?,
            self.read_float()?,
            self.read_float()?,
        ])
    }

    pub fn take_slice(&mut self, len: usize) -> &[u8] {
        let result = &self.buffer[0..len];
        self.buffer = &self.buffer[len..];
        self.pos += len;
        result
    }

    pub fn take_reader(&mut self, len: usize) -> DclReader {
        DclReader::new(self.take_slice(len))
    }

    pub fn as_slice(&self) -> &[u8] {
        self.buffer
    }
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn read<T: FromDclReader>(&mut self) -> Result<T, DclReaderError> {
        T::from_reader(self)
    }
}

// trait to build an object from a buffer stream
pub trait FromDclReader: Send + Sync + 'static {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError>
    where
        Self: Sized;
}
