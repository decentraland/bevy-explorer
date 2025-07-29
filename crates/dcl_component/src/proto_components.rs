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
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/decentraland.kernel.comms.v3.rs"));
        }
    }
}

#[allow(clippy::all)]
pub mod common {
    include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
}

pub mod social {
    include!(concat!(
        env!("OUT_DIR"),
        "/decentraland.social.friendships.rs"
    ));
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
impl DclProtoComponent for sdk::components::PbGltfNode {}
impl DclProtoComponent for sdk::components::PbGltfNodeState {}
impl DclProtoComponent for sdk::components::PbAvatarShape {}
impl DclProtoComponent for sdk::components::PbAvatarAttach {}
impl DclProtoComponent for sdk::components::PbAvatarBase {}
impl DclProtoComponent for sdk::components::PbAvatarEmoteCommand {}
impl DclProtoComponent for sdk::components::PbAvatarEquippedData {}
impl DclProtoComponent for sdk::components::PbPlayerIdentityData {}
impl DclProtoComponent for kernel::comms::rfc4::Packet {}
impl DclProtoComponent for sdk::components::PbUiCanvasInformation {}
impl DclProtoComponent for sdk::components::PbUiTransform {}
impl DclProtoComponent for sdk::components::PbUiText {}
impl DclProtoComponent for sdk::components::PbUiBackground {}
impl DclProtoComponent for sdk::components::PbUiInput {}
impl DclProtoComponent for sdk::components::PbUiInputResult {}
impl DclProtoComponent for sdk::components::PbUiDropdown {}
impl DclProtoComponent for sdk::components::PbUiDropdownResult {}
impl DclProtoComponent for sdk::components::PbUiScrollResult {}
impl DclProtoComponent for sdk::components::PbUiCanvas {}
impl DclProtoComponent for sdk::components::PbTextShape {}
impl DclProtoComponent for sdk::components::PbPointerLock {}
impl DclProtoComponent for sdk::components::PbCameraMode {}
impl DclProtoComponent for sdk::components::PbCameraModeArea {}
impl DclProtoComponent for sdk::components::PbAudioSource {}
impl DclProtoComponent for sdk::components::PbVideoPlayer {}
impl DclProtoComponent for sdk::components::PbAudioStream {}
impl DclProtoComponent for sdk::components::PbVideoEvent {}
impl DclProtoComponent for sdk::components::PbAudioEvent {}
impl DclProtoComponent for sdk::components::PbVisibilityComponent {}
impl DclProtoComponent for sdk::components::PbAvatarModifierArea {}
impl DclProtoComponent for sdk::components::PbNftShape {}
impl DclProtoComponent for sdk::components::PbTween {}
impl DclProtoComponent for sdk::components::PbTweenState {}
impl DclProtoComponent for sdk::components::PbLight {}
impl DclProtoComponent for sdk::components::PbSpotlight {}
impl DclProtoComponent for sdk::components::PbGlobalLight {}
impl DclProtoComponent for sdk::components::PbTextureCamera {}
impl DclProtoComponent for sdk::components::PbCameraLayers {}
impl DclProtoComponent for sdk::components::PbPrimaryPointerInfo {}
impl DclProtoComponent for sdk::components::PbCameraLayer {}
impl DclProtoComponent for sdk::components::PbRealmInfo {}
impl DclProtoComponent for sdk::components::PbVirtualCamera {}
impl DclProtoComponent for sdk::components::PbMainCamera {}
impl DclProtoComponent for sdk::components::PbInputModifier {}

// VECTOR2 conversions
impl Copy for common::Vector2 {}
impl From<bevy::prelude::Vec2> for common::Vector2 {
    fn from(value: bevy::prelude::Vec2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}
impl From<&common::Vector2> for bevy::prelude::Vec2 {
    fn from(value: &common::Vector2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

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

    pub fn abs_vec_to_vec3(&self) -> bevy::prelude::Vec3 {
        bevy::prelude::Vec3::new(self.x, self.y, self.z)
    }

    pub fn abs_vec_from_vec3(vec3: &bevy::prelude::Vec3) -> Self {
        Self {
            x: vec3.x,
            y: vec3.y,
            z: vec3.z,
        }
    }
}

// QUATERNION conversions
impl Copy for common::Quaternion {}
impl From<common::Quaternion> for bevy::math::Quat {
    fn from(q: common::Quaternion) -> Self {
        bevy::math::Quat::from_xyzw(q.x, q.y, -q.z, -q.w)
    }
}

// COLOR conversions
impl Copy for common::Color3 {}
impl Copy for common::Color4 {}

pub trait Color4DclToBevy {
    fn convert_linear_rgba(self) -> bevy::prelude::Color;
    fn convert_srgba(self) -> bevy::prelude::Color;
}

impl Color4DclToBevy for common::Color4 {
    fn convert_linear_rgba(self) -> bevy::prelude::Color {
        bevy::prelude::Color::linear_rgba(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
            self.a.clamp(0.0, 1.0),
        )
    }
    fn convert_srgba(self) -> bevy::prelude::Color {
        bevy::prelude::Color::srgba(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
            self.a.clamp(0.0, 1.0),
        )
    }
}

pub trait Color4BevyToDcl {
    fn convert_linear_rgba(self) -> common::Color4;
    fn convert_srgba(self) -> common::Color4;
}

impl Color4BevyToDcl for bevy::prelude::Color {
    fn convert_linear_rgba(self) -> common::Color4 {
        let rgba = self.to_linear();
        common::Color4 {
            r: rgba.red,
            g: rgba.green,
            b: rgba.blue,
            a: rgba.alpha,
        }
    }
    fn convert_srgba(self) -> common::Color4 {
        let rgba = self.to_srgba();
        common::Color4 {
            r: rgba.red,
            g: rgba.green,
            b: rgba.blue,
            a: rgba.alpha,
        }
    }
}

pub trait Color3DclToBevy {
    fn convert_linear_rgb(self) -> bevy::prelude::Color;
    fn convert_srgb(self) -> bevy::prelude::Color;
}

impl Color3DclToBevy for common::Color3 {
    fn convert_linear_rgb(self) -> bevy::prelude::Color {
        bevy::prelude::Color::linear_rgb(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
        )
    }
    fn convert_srgb(self) -> bevy::prelude::Color {
        bevy::prelude::Color::srgb(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
        )
    }
}

pub trait Color3BevyToDcl {
    fn convert_linear_rgb(self) -> common::Color3;
    fn convert_srgb(self) -> common::Color3;
}

impl Color3BevyToDcl for bevy::prelude::Color {
    fn convert_linear_rgb(self) -> common::Color3 {
        let rgba = self.to_linear();
        common::Color3 {
            r: rgba.red,
            g: rgba.green,
            b: rgba.blue,
        }
    }
    fn convert_srgb(self) -> common::Color3 {
        let rgba = self.to_srgba();
        common::Color3 {
            r: rgba.red,
            g: rgba.green,
            b: rgba.blue,
        }
    }
}

impl Copy for common::BorderRect {}
impl From<common::BorderRect> for bevy::prelude::UiRect {
    fn from(value: common::BorderRect) -> Self {
        Self {
            left: bevy::prelude::Val::Percent(value.left * 100.0),
            right: bevy::prelude::Val::Percent(value.right * 100.0),
            top: bevy::prelude::Val::Percent(value.top * 100.0),
            bottom: bevy::prelude::Val::Percent(value.bottom * 100.0),
        }
    }
}

// util for rounding, scenes expect near 0 to be == 0, etc
pub trait RoughRoundExt {
    fn round_at_pow2(self, pow2: i8) -> Self;
}

impl RoughRoundExt for bevy::math::Vec3 {
    fn round_at_pow2(self, pow2: i8) -> Self {
        (self * 2f32.powf(-pow2 as f32)).round() * 2f32.powf(pow2 as f32)
    }
}
