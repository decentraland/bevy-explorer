use super::FromDclReader;

include!(concat!(env!("OUT_DIR"), "/decentraland.sdk.components.rs"));

trait DclProtoComponent: prost::Message + Default {}

impl<T: DclProtoComponent + Sync + Send + 'static> FromDclReader for T {
    fn from_reader(buf: &mut super::DclReader) -> Result<Self, super::DclReaderError> {
        Ok(Self::decode(buf.as_slice())?)
    }
}

impl DclProtoComponent for PbBillboard {}
