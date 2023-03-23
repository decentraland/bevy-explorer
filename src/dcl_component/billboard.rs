use super::FromDclReader;
use prost::Message;

include!(concat!(env!("OUT_DIR"), "/dcl.billboard.rs"));

impl FromDclReader for PbBillboard {
    fn from_reader(buf: &mut super::DclReader) -> Result<Self, super::DclReaderError> {
        Ok(Self::decode(buf.as_slice())?)
    }
}
