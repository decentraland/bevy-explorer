use super::FromDclReader;

#[derive(Debug, Default, Clone)]
pub struct DclBillboard {
    pub axes: u8,
}

impl FromDclReader for DclBillboard {
    fn from_reader(buf: &mut super::DclReader) -> Result<Self, super::DclReaderError> {
        Ok(Self {
            axes: buf.read_u8()?,
        })
    }
}
