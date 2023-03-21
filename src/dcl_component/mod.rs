// structs representing dcl components and de/serialization
use bevy::prelude::Vec3;

pub mod billboard;
pub mod reader;
pub mod transform_and_parent;
pub mod writer;

pub use reader::{DclReader, DclReaderError, FromDclReader};
pub use writer::{DclWriter, ToDclWriter};

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy, Default)]
pub struct SceneEntityId {
    pub id: u16,
    pub generation: u16,
}
impl std::fmt::Display for SceneEntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("dcl_{}v{}", self.id, self.generation))
    }
}
impl SceneEntityId {
    const fn reserved(id: u16) -> Self {
        Self { id, generation: 0 }
    }

    pub const ROOT: SceneEntityId = Self::reserved(0);
    pub const PLAYER: SceneEntityId = Self::reserved(1);
    pub const CAMERA: SceneEntityId = Self::reserved(2);
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneComponentId(pub u32);

impl SceneComponentId {
    pub const TRANSFORM: SceneComponentId = SceneComponentId(1);
    pub const BILLBOARD: SceneComponentId = SceneComponentId(1090);
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneCrdtTimestamp(pub u32);

impl FromDclReader for Vec3 {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self::from_array(buf.read_float3()?))
    }
}

impl ToDclWriter for Vec3 {
    fn to_writer(&self, buf: &mut DclWriter) {
        buf.write_float3(&self.to_array())
    }
}

impl FromDclReader for SceneEntityId {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self {
            id: buf.read_u16()?,
            generation: buf.read_u16()?,
        })
    }
}

impl ToDclWriter for SceneEntityId {
    fn to_writer(&self, buf: &mut DclWriter) {
        buf.write_u16(self.id);
        buf.write_u16(self.generation);
    }
}

impl FromDclReader for SceneComponentId {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_u32()?))
    }
}

impl ToDclWriter for SceneComponentId {
    fn to_writer(&self, buf: &mut DclWriter) {
        buf.write_u32(self.0)
    }
}

impl FromDclReader for SceneCrdtTimestamp {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_u32()?))
    }
}

impl ToDclWriter for SceneCrdtTimestamp {
    fn to_writer(&self, buf: &mut DclWriter) {
        buf.write_u32(self.0)
    }
}
