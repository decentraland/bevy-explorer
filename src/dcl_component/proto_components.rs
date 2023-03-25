use bevy::prelude::Vec3;

use super::FromDclReader;

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

impl DclProtoComponent for sdk::components::PbBillboard {}
impl DclProtoComponent for sdk::components::PbRaycast {}
impl DclProtoComponent for sdk::components::PbMeshRenderer {}
impl DclProtoComponent for sdk::components::PbMeshCollider {}

impl From<&common::Vector3> for Vec3 {
    fn from(f: &common::Vector3) -> Self {
        Vec3 {
            x: f.x,
            y: f.y,
            z: f.z,
        }
    }
}

impl From<common::Vector3> for Vec3 {
    fn from(f: common::Vector3) -> Self {
        Vec3 {
            x: f.x,
            y: f.y,
            z: f.z,
        }
    }
}
