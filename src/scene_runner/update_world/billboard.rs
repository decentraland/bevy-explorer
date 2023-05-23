// implements billboard transforms.
// NOTE: this implementation is not fully correct: we use the current global transform to set the billboard
// component's local transform, but global transforms are only upated at the end of the frame. so, a chain
// of X billboards parented together will have a latency of X frames (with no target movement) before the final
// member is guaranteed to be oriented correctly
use bevy::math::Vec3Swizzles;
use bevy::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{proto_components::sdk::components::PbBillboard, SceneComponentId},
    scene_runner::SceneSets,
    user_input::camera::PrimaryCamera,
};

use super::AddCrdtInterfaceExt;

pub struct BillboardPlugin;

impl Plugin for BillboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbBillboard, Billboard>(
            SceneComponentId::BILLBOARD,
            ComponentPosition::EntityOnly,
        );

        app.add_system(update_billboards.in_set(SceneSets::PostLoop));
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Component, PartialEq, Eq)]
pub enum Billboard {
    None,
    Y,
    YX,
    All,
}

impl From<Option<i32>> for Billboard {
    fn from(value: Option<i32>) -> Self {
        match value {
            Some(0) => Billboard::None,
            Some(2) => Billboard::Y,
            Some(3) => Billboard::YX,
            _ => Billboard::All,
        }
    }
}

impl From<PbBillboard> for Billboard {
    fn from(value: PbBillboard) -> Self {
        value.billboard_mode.into()
    }
}

pub(crate) fn update_billboards(
    global_transforms: Query<&GlobalTransform>,
    mut q: Query<(&mut Transform, &GlobalTransform, &Billboard, &Parent)>,
    cam: Query<&GlobalTransform, With<PrimaryCamera>>,
) {
    let Ok(cam_global_transform) = cam.get_single() else {
        // no camera, no billboard
        return;
    };
    let (_, cam_g_rotation, cam_g_translation) =
        cam_global_transform.to_scale_rotation_translation();
    let cam_z = cam_g_rotation.to_euler(EulerRot::YXZ).2;

    for (mut local_transform, global_transform, billboard, parent) in q.iter_mut() {
        // get reference frame
        let frame = global_transforms.get(parent.get()).unwrap();

        match billboard {
            Billboard::None => (),
            Billboard::All => {
                // use global frame of reference
                let (g_scale, _, g_translation) = global_transform.to_scale_rotation_translation();
                let cam_direction = cam_g_translation - g_translation;
                let target_global_rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    cam_direction.x.atan2(cam_direction.z),
                    -cam_direction.y.atan2(cam_direction.xz().length()),
                    cam_z,
                );
                let target_global_transform = Transform {
                    translation: g_translation,
                    rotation: target_global_rotation,
                    scale: g_scale,
                };
                let target_local_matrix =
                    frame.compute_matrix().inverse() * target_global_transform.compute_matrix();
                let target_transform = Transform::from_matrix(target_local_matrix);

                // just update the rotation so that scale and translation don't drift, or change on first frame if GlobalTransform is not yet updated
                local_transform.rotation = target_transform.rotation;
            }
            Billboard::Y | Billboard::YX => {
                // map camera into local frame
                // TODO use GlobalTransform::raparented_to
                let cam_local_matrix =
                    frame.compute_matrix().inverse() * cam_global_transform.compute_matrix();
                let (_, _, cam_local_translation) =
                    cam_local_matrix.to_scale_rotation_translation();

                let cam_direction = cam_local_translation - local_transform.translation;
                let mut euler_angles = local_transform.rotation.to_euler(EulerRot::YXZ);

                // rotate to face / yaw
                euler_angles.0 = cam_direction.x.atan2(cam_direction.z);

                if billboard == &Billboard::YX {
                    // tilt to face / pitch
                    euler_angles.1 = -cam_direction.y.atan2(cam_direction.xz().length());
                }

                local_transform.rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    euler_angles.0,
                    euler_angles.1,
                    euler_angles.2,
                );
            }
        }
    }
}
