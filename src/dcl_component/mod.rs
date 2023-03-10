use bevy::prelude::Vec3;

mod reader;
pub mod transform_and_parent;

pub use reader::{DclReader, DclReaderError, FromDclReader};

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneEntityId(pub u32);
impl SceneEntityId {
    pub const ROOT: SceneEntityId = SceneEntityId(0);
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneComponentId(pub u32);

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneCrdtTimestamp(pub u32);

impl FromDclReader for Vec3 {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self::from_array(buf.read_float3()?))
    }
}

impl FromDclReader for SceneEntityId {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_u32()?))
    }
}

impl FromDclReader for SceneComponentId {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_u32()?))
    }
}

impl FromDclReader for SceneCrdtTimestamp {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_u32()?))
    }
}
