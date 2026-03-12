use std::ops::{Add, Sub};

use bevy::prelude::{Quat, Transform, Vec3};

use super::{DclReader, DclReaderError, FromDclReader, PositionFree, SceneEntityId, ToDclWriter};

// for dcl: +z -> forward
// for bevy: +z -> backward
// DclTranslation internal format is wire format (+z = forward)
#[derive(Debug, Default, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DclTranslation(pub [f32; 3]);

impl FromDclReader for DclTranslation {
    fn from_reader(reader: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(reader.read_float3()?))
    }
}

impl ToDclWriter for DclTranslation {
    fn to_writer(&self, buf: &mut super::DclWriter) {
        buf.write_float3(&self.0)
    }
}

impl DclTranslation {
    #[allow(dead_code)]
    pub fn from_bevy_translation(rh_vec: Vec3) -> Self {
        Self([rh_vec.x, rh_vec.y, -rh_vec.z])
    }

    pub fn to_bevy_translation(self) -> Vec3 {
        Vec3::new(self.0[0], self.0[1], -self.0[2])
    }
}

impl Add<DclTranslation> for DclTranslation {
    type Output = Self;

    fn add(self, rhs: DclTranslation) -> Self::Output {
        Self([
            self.0[0] + rhs.0[0],
            self.0[1] + rhs.0[1],
            self.0[2] + rhs.0[2],
        ])
    }
}

impl Sub<DclTranslation> for DclTranslation {
    type Output = Self;

    fn sub(self, rhs: DclTranslation) -> Self::Output {
        Self([
            self.0[0] - rhs.0[0],
            self.0[1] - rhs.0[1],
            self.0[2] - rhs.0[2],
        ])
    }
}

// internal format is wire format (+z = forward)
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DclQuat(pub [f32; 4]);

impl FromDclReader for DclQuat {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_float4()?))
    }
}

impl ToDclWriter for DclQuat {
    fn to_writer(&self, buf: &mut super::DclWriter) {
        buf.write_float4(&self.0)
    }
}

impl Default for DclQuat {
    fn default() -> Self {
        Self::from_bevy_quat(Quat::default())
    }
}

impl DclQuat {
    // to mirror a quaternion, we can either invert the mirror plane basis components or invert the mirrored component and the scalar component
    // here we invert mirrored plus scalar

    #[allow(dead_code)]
    pub fn from_bevy_quat(rh_quat: Quat) -> Self {
        Self([rh_quat.x, rh_quat.y, -rh_quat.z, -rh_quat.w])
    }

    pub fn to_bevy_quat(self) -> Quat {
        Quat::from_xyzw(self.0[0], self.0[1], -self.0[2], -self.0[3])
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(from = "DclTransformAndParentJson", into = "DclTransformAndParentJson")]
pub struct DclTransformAndParent {
    pub translation: DclTranslation,
    pub rotation: DclQuat,
    pub scale: Vec3,
    pub parent: SceneEntityId,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct XYZ {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct XYZW {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DclTransformAndParentJson {
    position: XYZ,
    rotation: XYZW,
    scale: XYZ,
    parent: u32,
}

impl From<DclTransformAndParent> for DclTransformAndParentJson {
    fn from(t: DclTransformAndParent) -> Self {
        Self {
            position: XYZ {
                x: t.translation.0[0],
                y: t.translation.0[1],
                z: t.translation.0[2],
            },
            rotation: XYZW {
                x: t.rotation.0[0],
                y: t.rotation.0[1],
                z: t.rotation.0[2],
                w: t.rotation.0[3],
            },
            scale: XYZ {
                x: t.scale.x,
                y: t.scale.y,
                z: t.scale.z,
            },
            parent: t.parent.as_proto_u32().unwrap_or(0),
        }
    }
}

impl From<DclTransformAndParentJson> for DclTransformAndParent {
    fn from(j: DclTransformAndParentJson) -> Self {
        Self {
            translation: DclTranslation([j.position.x, j.position.y, j.position.z]),
            rotation: DclQuat([j.rotation.x, j.rotation.y, j.rotation.z, j.rotation.w]),
            scale: Vec3::new(j.scale.x, j.scale.y, j.scale.z),
            parent: SceneEntityId::from_proto_u32(j.parent),
        }
    }
}

impl DclTransformAndParent {
    pub fn to_bevy_transform(&self) -> Transform {
        let rotation = self.rotation.to_bevy_quat().normalize();
        let rotation = if rotation.is_finite() {
            rotation
        } else {
            bevy::prelude::Quat::IDENTITY
        };

        let mut scale = self.scale;
        if scale.x == 0.0 {
            scale.x = f32::EPSILON;
        };
        if scale.y == 0.0 {
            scale.y = f32::EPSILON;
        };
        if scale.z == 0.0 {
            scale.z = f32::EPSILON;
        };

        Transform {
            translation: self.translation.to_bevy_translation(),
            rotation,
            scale,
        }
    }

    pub fn parent(&self) -> SceneEntityId {
        self.parent
    }

    pub fn from_bevy_transform_and_parent(transform: &Transform, parent: SceneEntityId) -> Self {
        Self {
            translation: DclTranslation::from_bevy_translation(transform.translation),
            rotation: DclQuat::from_bevy_quat(transform.rotation),
            scale: transform.scale,
            parent,
        }
    }
}

impl FromDclReader for DclTransformAndParent {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(DclTransformAndParent {
            translation: buf.read()?,
            rotation: buf.read()?,
            scale: buf.read()?,
            parent: buf.read()?,
        })
    }
}

impl ToDclWriter for DclTransformAndParent {
    fn to_writer(&self, buf: &mut super::DclWriter) {
        buf.write(&self.translation);
        buf.write(&self.rotation);
        buf.write(&self.scale);
        buf.write(&self.parent);
    }
}

// Transforms are localized via WORLD_ORIGIN parenting, not payload adjustment
impl PositionFree for DclTransformAndParent {}
