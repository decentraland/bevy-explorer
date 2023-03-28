use std::ops::Deref;

use super::DclReader;

pub struct DclWriter<'a> {
    buffer: &'a mut Vec<u8>,
}

impl<'a> DclWriter<'a> {
    pub fn new(buffer: &'a mut Vec<u8>) -> Self {
        Self { buffer }
    }

    pub fn write_raw(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data)
    }

    pub fn write_u16(&mut self, value: u16) {
        self.write_raw(&value.to_le_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.write_raw(&value.to_le_bytes());
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

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn reader(&self) -> DclReader {
        DclReader::new(self.buffer)
    }
}

impl<'a> Deref for DclWriter<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

pub trait ToDclWriter {
    fn to_writer(&self, buf: &mut DclWriter);
}

unsafe impl<'a> prost::bytes::BufMut for DclWriter<'a> {
    fn remaining_mut(&self) -> usize {
        self.buffer.remaining_mut()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.buffer.advance_mut(cnt)
    }

    fn chunk_mut(&mut self) -> &mut prost::bytes::buf::UninitSlice {
        self.buffer.chunk_mut()
    }
}
