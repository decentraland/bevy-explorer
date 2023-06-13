use std::ops::RangeInclusive;

// structs representing dcl components and de/serialization
use bevy::prelude::Vec3;

pub mod proto_components;
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
    pub const WORLD_ORIGIN: SceneEntityId = Self::reserved(5);

    pub const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=405;

    pub fn as_proto_u32(&self) -> Option<u32> {
        Some(self.id as u32 | (self.generation as u32) << 16)
    }

    pub fn from_proto_u32(id: u32) -> Self {
        SceneEntityId {
            id: id as u16,
            generation: (id >> 16) as u16,
        }
    }

    pub fn new(id: u16, generation: u16) -> Self {
        Self { id, generation }
    }
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SceneComponentId(pub u32);

impl SceneComponentId {
    pub const TRANSFORM: SceneComponentId = SceneComponentId(1);

    pub const MATERIAL: SceneComponentId = SceneComponentId(1017);
    pub const MESH_RENDERER: SceneComponentId = SceneComponentId(1018);
    pub const MESH_COLLIDER: SceneComponentId = SceneComponentId(1019);

    pub const GLTF_CONTAINER: SceneComponentId = SceneComponentId(1041);
    pub const ANIMATOR: SceneComponentId = SceneComponentId(1042);

    pub const ENGINE_INFO: SceneComponentId = SceneComponentId(1048);
    pub const GLTF_CONTAINER_LOADING_STATE: SceneComponentId = SceneComponentId(1049);

    pub const UI_TRANSFORM: SceneComponentId = SceneComponentId(1050);
    pub const UI_TEXT: SceneComponentId = SceneComponentId(1052);
    pub const UI_BACKGROUND: SceneComponentId = SceneComponentId(1053);

    pub const CANVAS_INFO: SceneComponentId = SceneComponentId(1054);

    pub const POINTER_EVENTS: SceneComponentId = SceneComponentId(1062);
    pub const POINTER_RESULT: SceneComponentId = SceneComponentId(1063);

    pub const RAYCAST: SceneComponentId = SceneComponentId(1067);
    pub const RAYCAST_RESULT: SceneComponentId = SceneComponentId(1068);

    pub const AVATAR_ATTACHMENT: SceneComponentId = SceneComponentId(1073);
    pub const AVATAR_SHAPE: SceneComponentId = SceneComponentId(1080);
    pub const AVATAR_CUSTOMIZATION: SceneComponentId = SceneComponentId(1087);
    pub const AVATAR_EMOTE_COMMAND: SceneComponentId = SceneComponentId(1088);
    pub const AVATAR_EQUIPPED_DATA: SceneComponentId = SceneComponentId(1089);

    pub const BILLBOARD: SceneComponentId = SceneComponentId(1090);
    pub const PLAYER_IDENTITY_DATA: SceneComponentId = SceneComponentId(1091);

    pub const UI_INPUT: SceneComponentId = SceneComponentId(1093);
    pub const UI_INPUT_RESULT: SceneComponentId = SceneComponentId(1095);
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
