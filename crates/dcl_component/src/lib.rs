use std::ops::RangeInclusive;

// structs representing dcl components and de/serialization
use bevy::prelude::Vec3;

pub mod component_name_registry;
pub mod component_schema;
pub mod crdt_type;
pub mod proto_components;
pub mod reader;
pub mod transform_and_parent;
pub mod writer;

pub use component_name_registry::ComponentNameRegistry;
pub use crdt_type::{ComponentPosition, CrdtType};
pub use reader::{DclReader, DclReaderError, FromDclReader};
use serde::{Deserialize, Serialize};
pub use writer::{DclWriter, ToDclWriter};

/// Scene origin in DCL proto-space (z-forward), stored in scene thread state for localizer access.
pub struct SceneOrigin(pub Vec3);

/// Describes how to localize a component's position data when delivering to a scene.
/// Each variant corresponds to a known component layout so the receiver can
/// deserialize, adjust, and re-encode position fields relative to the scene origin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Localizer {
    /// No localization needed (component contains no position data).
    None,
    /// Localization strategy not yet defined. Acceptable at scene startup (initial CRDT store
    /// may contain pre-localized static data), but will cause an error at global update receipt.
    Unimplemented,
    /// Localize `PbAvatarMovementInfo`: offset `walk_target` (field 8) by scene origin.
    AvatarMovementInfo,
    /// Localize `DclTransformAndParent`: world-space transforms (parented to `WORLD_ORIGIN`)
    /// get their translation offset by scene origin and are re-parented to `ROOT`, so scenes
    /// read scene-relative positions for foreign players.
    Transform,
}

impl Localizer {
    /// Localize a proto component payload by deserializing, adjusting position
    /// fields, and re-encoding. `scene_origin` is in DCL proto-space (z-forward).
    pub fn localize_payload(&self, payload: &[u8], scene_origin: &SceneOrigin) -> Vec<u8> {
        match self {
            Localizer::None => payload.to_vec(),
            Localizer::Unimplemented => payload.to_vec(),
            Localizer::AvatarMovementInfo => {
                use prost::Message;
                use proto_components::sdk::components::PbAvatarMovementInfo;

                let Ok(mut info) = PbAvatarMovementInfo::decode(payload) else {
                    return payload.to_vec();
                };

                let origin = &scene_origin.0;

                // walk_target is a world-space position → make scene-relative
                if let Some(ref mut target) = info.walk_target {
                    target.x -= origin.x;
                    target.y -= origin.y;
                    target.z -= origin.z;
                }

                let mut buf = Vec::with_capacity(payload.len());
                info.encode(&mut buf).expect("re-encode failed");
                buf
            }
            Localizer::Transform => {
                use transform_and_parent::DclTransformAndParent;

                let mut reader = DclReader::new(payload);
                let Ok(mut transform) = reader.read::<DclTransformAndParent>() else {
                    return payload.to_vec();
                };

                // Only WORLD_ORIGIN-parented transforms carry world-space positions;
                // anything else is already in scene space.
                if transform.parent != SceneEntityId::WORLD_ORIGIN {
                    return payload.to_vec();
                }

                let origin = &scene_origin.0;
                transform.translation.0[0] -= origin.x;
                transform.translation.0[1] -= origin.y;
                transform.translation.0[2] -= origin.z;
                transform.parent = SceneEntityId::ROOT;

                let mut buf = Vec::with_capacity(payload.len());
                DclWriter::new(&mut buf).write(&transform);
                buf
            }
        }
    }
}

/// Trait for types that can be sent via `GlobalCrdtState::update_crdt`.
/// Provides type-safe enforcement that localization has been considered.
pub trait GlobalCrdtData: ToDclWriter {
    fn localizer() -> Localizer;
}

/// Marker trait for types sent via global CRDT that contain no position data.
/// Automatically implements `GlobalCrdtData` with `Localizer::None`.
pub trait PositionFree: ToDclWriter {}

impl<T: PositionFree> GlobalCrdtData for T {
    fn localizer() -> Localizer {
        Localizer::None
    }
}

#[derive(
    PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy, Default, Serialize, Deserialize,
)]
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

    /// size for a table indexed by `id`: both 0 and 65535 are valid indexes
    pub const LIVE_TABLE_LEN: usize = u16::MAX as usize + 1;

    pub fn as_proto_u32(&self) -> Option<u32> {
        Some(self.id as u32 | ((self.generation as u32) << 16))
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

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SceneComponentId(pub u32);

impl SceneComponentId {
    pub const TRANSFORM: SceneComponentId = SceneComponentId(1);

    pub const MATERIAL: SceneComponentId = SceneComponentId(1017);
    pub const MESH_RENDERER: SceneComponentId = SceneComponentId(1018);
    pub const MESH_COLLIDER: SceneComponentId = SceneComponentId(1019);
    pub const AUDIO_SOURCE: SceneComponentId = SceneComponentId(1020);
    pub const AUDIO_STREAM: SceneComponentId = SceneComponentId(1021);

    pub const TEXT_SHAPE: SceneComponentId = SceneComponentId(1030);
    pub const NFT_SHAPE: SceneComponentId = SceneComponentId(1040);

    pub const GLTF_CONTAINER: SceneComponentId = SceneComponentId(1041);
    pub const ANIMATOR: SceneComponentId = SceneComponentId(1042);
    pub const GLTF_NODE_MODIFIERS: SceneComponentId = SceneComponentId(1099);

    pub const VIDEO_PLAYER: SceneComponentId = SceneComponentId(1043);
    pub const VIDEO_EVENT: SceneComponentId = SceneComponentId(1044);
    pub const AUDIO_EVENT: SceneComponentId = SceneComponentId(1105);

    pub const GLTF_NODE: SceneComponentId = SceneComponentId(1200);
    pub const GLTF_NODE_STATE: SceneComponentId = SceneComponentId(1201);

    pub const ENGINE_INFO: SceneComponentId = SceneComponentId(1048);
    pub const GLTF_CONTAINER_LOADING_STATE: SceneComponentId = SceneComponentId(1049);

    pub const UI_TRANSFORM: SceneComponentId = SceneComponentId(1050);
    pub const UI_TEXT: SceneComponentId = SceneComponentId(1052);
    pub const UI_BACKGROUND: SceneComponentId = SceneComponentId(1053);

    pub const CANVAS_INFO: SceneComponentId = SceneComponentId(1054);
    pub const UI_CANVAS: SceneComponentId = SceneComponentId(1203);

    pub const TRIGGER_AREA: SceneComponentId = SceneComponentId(1060);
    pub const TRIGGER_AREA_RESULT: SceneComponentId = SceneComponentId(1061);

    pub const POINTER_EVENTS: SceneComponentId = SceneComponentId(1062);
    pub const POINTER_RESULT: SceneComponentId = SceneComponentId(1063);

    pub const RAYCAST: SceneComponentId = SceneComponentId(1067);
    pub const RAYCAST_RESULT: SceneComponentId = SceneComponentId(1068);

    pub const AVATAR_MODIFIER_AREA: SceneComponentId = SceneComponentId(1070);
    pub const INPUT_MODIFIER: SceneComponentId = SceneComponentId(1078);

    pub const CAMERA_MODE_AREA: SceneComponentId = SceneComponentId(1071);
    pub const CAMERA_MODE: SceneComponentId = SceneComponentId(1072);
    pub const MAIN_CAMERA: SceneComponentId = SceneComponentId(1075);
    pub const VIRTUAL_CAMERA: SceneComponentId = SceneComponentId(1076);

    pub const AVATAR_ATTACHMENT: SceneComponentId = SceneComponentId(1073);

    pub const POINTER_LOCK: SceneComponentId = SceneComponentId(1074);

    pub const AVATAR_SHAPE: SceneComponentId = SceneComponentId(1080);

    pub const VISIBILITY: SceneComponentId = SceneComponentId(1081);

    pub const AVATAR_BASE: SceneComponentId = SceneComponentId(1087);
    pub const AVATAR_EMOTE_COMMAND: SceneComponentId = SceneComponentId(1088);
    pub const AVATAR_EQUIPPED_DATA: SceneComponentId = SceneComponentId(1091);

    pub const BILLBOARD: SceneComponentId = SceneComponentId(1090);
    pub const PLAYER_IDENTITY_DATA: SceneComponentId = SceneComponentId(1089);

    pub const UI_INPUT: SceneComponentId = SceneComponentId(1093);
    pub const UI_DROPDOWN: SceneComponentId = SceneComponentId(1094);
    pub const UI_INPUT_RESULT: SceneComponentId = SceneComponentId(1095);
    pub const UI_DROPDOWN_RESULT: SceneComponentId = SceneComponentId(1096);
    pub const UI_SCROLL_RESULT: SceneComponentId = SceneComponentId(1202);

    pub const TWEEN: SceneComponentId = SceneComponentId(1102);
    pub const TWEEN_STATE: SceneComponentId = SceneComponentId(1103);

    pub const LIGHT_SOURCE: SceneComponentId = SceneComponentId(1079);
    pub const GLOBAL_LIGHT: SceneComponentId = SceneComponentId(1206);
    pub const TEXTURE_CAMERA: SceneComponentId = SceneComponentId(1207);
    pub const CAMERA_LAYERS: SceneComponentId = SceneComponentId(1208);
    pub const PRIMARY_POINTER_INFO: SceneComponentId = SceneComponentId(1209);
    pub const SKYBOX_TIME: SceneComponentId = SceneComponentId(1210);
    pub const CAMERA_LAYER: SceneComponentId = SceneComponentId(1503);

    pub const REALM_INFO: SceneComponentId = SceneComponentId(1106);

    pub const AVATAR_MOVEMENT_INFO: SceneComponentId = SceneComponentId(1500);
    pub const AVATAR_MOVEMENT: SceneComponentId = SceneComponentId(1501);
    pub const AVATAR_LOCOMOTION_SETTINGS: SceneComponentId = SceneComponentId(1211);

    pub const ASSET_LOAD: SceneComponentId = SceneComponentId(1213);
    pub const ASSET_LOAD_LOADING_STATE: SceneComponentId = SceneComponentId(1214);

    pub const PHYSICS_COMBINED_IMPULSE: SceneComponentId = SceneComponentId(1215);
    pub const PHYSICS_COMBINED_FORCE: SceneComponentId = SceneComponentId(1216);

    pub const PARTICLE_SYSTEM: SceneComponentId = SceneComponentId(1217);
}

#[derive(
    PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Clone, Copy, Default, Serialize, Deserialize,
)]
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

#[cfg(test)]
mod test {
    use super::*;
    use transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation};

    fn encode(transform: &DclTransformAndParent) -> Vec<u8> {
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(transform);
        buf
    }

    #[test]
    fn transform_localizer_offsets_world_origin_transforms() {
        let payload = encode(&DclTransformAndParent {
            translation: DclTranslation([1456.4, 0.13, -135.1]),
            rotation: DclQuat([0.0, 0.0, 0.0, 1.0]),
            scale: Vec3::ONE,
            parent: SceneEntityId::WORLD_ORIGIN,
        });
        let origin = SceneOrigin(Vec3::new(1440.0, 0.0, -144.0));

        let localized = Localizer::Transform.localize_payload(&payload, &origin);
        let result: DclTransformAndParent = DclReader::new(&localized).read().unwrap();

        assert_eq!(result.parent, SceneEntityId::ROOT);
        assert!((result.translation.0[0] - 16.4).abs() < 1e-3);
        assert!((result.translation.0[1] - 0.13).abs() < 1e-6);
        assert!((result.translation.0[2] - 8.9).abs() < 1e-3);
    }

    #[test]
    fn transform_localizer_leaves_scene_space_transforms_untouched() {
        let payload = encode(&DclTransformAndParent {
            translation: DclTranslation([1.0, 2.0, 3.0]),
            rotation: DclQuat([0.0, 0.0, 0.0, 1.0]),
            scale: Vec3::ONE,
            parent: SceneEntityId::ROOT,
        });
        let origin = SceneOrigin(Vec3::new(160.0, 0.0, 320.0));

        assert_eq!(
            Localizer::Transform.localize_payload(&payload, &origin),
            payload
        );
    }

    #[test]
    fn transform_localizer_passes_through_malformed_payloads() {
        let origin = SceneOrigin(Vec3::new(160.0, 0.0, 320.0));
        let truncated = vec![0u8; 10];

        assert_eq!(
            Localizer::Transform.localize_payload(&truncated, &origin),
            truncated
        );
    }
}
