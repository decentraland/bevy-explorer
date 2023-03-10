use bevy::prelude::{Quat, Transform, Vec3};

use super::{DclReader, DclReaderError, FromDclReader, SceneEntityId};

#[derive(Debug)]
pub struct DclTranslation([f32; 3]);

impl FromDclReader for DclTranslation {
    fn from_reader(reader: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(reader.read_float3()?))
    }
}

impl DclTranslation {
    // for dcl: +z -> forward
    // for bevy: +z -> backward

    #[allow(dead_code)]
    pub fn from_bevy_translation(rh_vec: Vec3) -> Self {
        Self([rh_vec.x, rh_vec.y, -rh_vec.z])
    }

    pub fn to_bevy_translation(&self) -> Vec3 {
        Vec3::new(self.0[0], self.0[1], -self.0[2])
    }
}

#[derive(Debug)]
pub struct DclQuat([f32; 4]);

impl FromDclReader for DclQuat {
    fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(Self(buf.read_float4()?))
    }
}

impl DclQuat {
    // to mirror a quaternion, we can either invert the mirror plane basis components or invert the mirrored component and the scalar component
    // here we invert mirrored plus scalar

    #[allow(dead_code)]
    pub fn from_bevy_quat(rh_quat: Quat) -> Self {
        Self([rh_quat.x, rh_quat.y, -rh_quat.z, -rh_quat.w])
    }

    pub fn to_bevy_quat(&self) -> Quat {
        Quat::from_xyzw(self.0[0], self.0[1], -self.0[2], -self.0[3])
    }
}

#[derive(Debug)]
pub struct DclTransformAndParent {
    translation: DclTranslation,
    rotation: DclQuat,
    scale: Vec3,
    parent: SceneEntityId,
}

impl DclTransformAndParent {
    pub fn to_bevy_transform(&self) -> Transform {
        Transform {
            translation: self.translation.to_bevy_translation(),
            rotation: self.rotation.to_bevy_quat(),
            scale: self.scale,
        }
    }

    pub fn parent(&self) -> SceneEntityId {
        self.parent
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
