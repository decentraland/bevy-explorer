use std::ops::Deref;

use super::DclReader;

pub struct DclWriter {
    buffer: Vec<u8>,
}

impl DclWriter {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    pub fn write_raw(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data)
    }

    pub fn write_u16(&mut self, value: u16) {
        self.write_raw(&value.to_be_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.write_raw(&value.to_be_bytes());
    }

    pub fn write_float(&mut self, value: f32) {
        self.write_u32(value.to_bits())
    }

    pub fn write_float3(&mut self, value: &[f32; 3]) {
        self.write_float(value[0]);
        self.write_float(value[1]);
        self.write_float(value[2]);
    }

    pub fn write_float4(&mut self, value: &[f32; 4]) {
        self.write_float(value[0]);
        self.write_float(value[1]);
        self.write_float(value[2]);
        self.write_float(value[3]);
    }

    pub fn write<T: ToDclWriter>(&mut self, value: &T) {
        value.to_writer(self)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn reader(&self) -> DclReader {
        DclReader::new(&self.buffer)
    }
}

impl From<DclWriter> for Vec<u8> {
    fn from(value: DclWriter) -> Self {
        value.buffer
    }
}

impl Deref for DclWriter {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

pub trait ToDclWriter {
    fn to_writer(&self, buf: &mut DclWriter);
}
