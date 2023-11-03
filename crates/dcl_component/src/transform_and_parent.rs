use std::ops::{Add, Sub};

use bevy::prelude::{Quat, Transform, Vec3};

use super::{DclReader, DclReaderError, FromDclReader, SceneEntityId, ToDclWriter};

// for dcl: +z -> forward
// for bevy: +z -> backward
// DclTranslation internal format is wire format (+z = forward)
#[derive(Debug, Default, Clone, Copy)]
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
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Default, Clone)]
pub struct DclTransformAndParent {
    pub translation: DclTranslation,
    pub rotation: DclQuat,
    pub scale: Vec3,
    pub parent: SceneEntityId,
}

impl DclTransformAndParent {
    pub fn to_bevy_transform(&self) -> Transform {
        let rotation = self.rotation.to_bevy_quat().normalize();
        let rotation = if rotation.is_finite() {
            rotation
        } else {
            bevy::prelude::Quat::IDENTITY
        };

        Transform {
            translation: self.translation.to_bevy_translation(),
            rotation,
            scale: self.scale,
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
