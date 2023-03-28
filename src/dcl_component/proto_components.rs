use bevy::prelude::Vec3;

use super::{FromDclReader, ToDclWriter};

pub mod sdk {
    #[allow(clippy::all)]
    pub mod components {
        include!(concat!(env!("OUT_DIR"), "/decentraland.sdk.components.rs"));

        pub mod common {
            include!(concat!(
                env!("OUT_DIR"),
                "/decentraland.sdk.components.common.rs"
            ));
        }
    }
}

pub mod common {
    include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
}

trait DclProtoComponent: prost::Message + Default {}

impl<T: DclProtoComponent + Sync + Send + 'static> FromDclReader for T {
    fn from_reader(buf: &mut super::DclReader) -> Result<Self, super::DclReaderError> {
        Ok(Self::decode(buf.as_slice())?)
    }
}

impl<T: DclProtoComponent + Sync + Send + 'static> ToDclWriter for T {
    fn to_writer(&self, buf: &mut super::DclWriter) {
        self.encode(buf).unwrap()
    }
}

// TODO check if generic T impl where T: prost::Message works
// i think it might break the primitive impls
impl DclProtoComponent for sdk::components::PbBillboard {}
impl DclProtoComponent for sdk::components::PbRaycast {}
impl DclProtoComponent for sdk::components::PbRaycastResult {}
impl DclProtoComponent for sdk::components::PbMeshRenderer {}
impl DclProtoComponent for sdk::components::PbMeshCollider {}

impl Copy for common::Vector3 {}
impl std::ops::Mul<f32> for common::Vector3 {
    type Output = common::Vector3;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}
impl std::ops::Add<common::Vector3> for common::Vector3 {
    type Output = common::Vector3;

    fn add(self, rhs: common::Vector3) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl common::Vector3 {
    // flip z coordinate for handedness
    pub fn world_vec_to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, -self.z)
    }

    pub fn world_vec_from_vec3(vec3: &Vec3) -> Self {
        Self {
            x: vec3.x,
            y: vec3.y,
            z: -vec3.z,
        }
    }
}
