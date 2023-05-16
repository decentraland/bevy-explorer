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

pub mod kernel {
    #[allow(clippy::all)]
    pub mod comms {
        pub mod rfc5 {
            include!(concat!(
                env!("OUT_DIR"),
                "/decentraland.kernel.comms.rfc5.rs"
            ));
        }
        pub mod rfc4 {
            include!(concat!(
                env!("OUT_DIR"),
                "/decentraland.kernel.comms.rfc4.rs"
            ));
        }
    }
}

#[allow(clippy::all)]
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
impl DclProtoComponent for sdk::components::PbMaterial {}
impl DclProtoComponent for sdk::components::PbGltfContainer {}
impl DclProtoComponent for sdk::components::PbAnimator {}
impl DclProtoComponent for sdk::components::PbPointerEvents {}
impl DclProtoComponent for sdk::components::PbPointerEventsResult {}
impl DclProtoComponent for sdk::components::PbEngineInfo {}
impl DclProtoComponent for sdk::components::PbGltfContainerLoadingState {}
impl DclProtoComponent for sdk::components::PbAvatarShape {}
impl DclProtoComponent for sdk::components::PbAvatarAttach {}
impl DclProtoComponent for sdk::components::PbAvatarCustomization {}
impl DclProtoComponent for sdk::components::PbAvatarEmoteCommand {}
impl DclProtoComponent for sdk::components::PbAvatarEquippedData {}
impl DclProtoComponent for sdk::components::PbPlayerIdentityData {}
impl DclProtoComponent for kernel::comms::rfc4::Packet {}

// VECTOR3 conversions
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
    pub fn world_vec_to_vec3(&self) -> bevy::prelude::Vec3 {
        bevy::prelude::Vec3::new(self.x, self.y, -self.z)
    }

    pub fn world_vec_from_vec3(vec3: &bevy::prelude::Vec3) -> Self {
        Self {
            x: vec3.x,
            y: vec3.y,
            z: -vec3.z,
        }
    }
}

// COLOR conversions
impl Copy for common::Color3 {}
impl Copy for common::Color4 {}
impl From<common::Color4> for bevy::prelude::Color {
    fn from(value: common::Color4) -> Self {
        bevy::prelude::Color::rgba(value.r, value.g, value.b, value.a)
    }
}

impl From<common::Color3> for bevy::prelude::Color {
    fn from(value: common::Color3) -> Self {
        bevy::prelude::Color::rgb(value.r, value.g, value.b)
    }
}
