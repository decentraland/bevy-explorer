// implements billboard transforms.
// NOTE: this implementation is not fully correct: we use the current global transform to set the billboard
// component's local transform, but global transforms are only upated at the end of the frame. so, a chain
// of X billboards parented together will have a latency of X frames (with no target movement) before the final
// member is guaranteed to be oriented correctly
use bevy::math::Vec3Swizzles;
use bevy::prelude::*;

use common::{sets::PostUpdateSets, structs::PrimaryCamera};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::PbBillboard, SceneComponentId, SceneEntityId,
};

use crate::{RendererSceneContext, SceneEntity};

use super::AddCrdtInterfaceExt;

pub struct BillboardPlugin;

impl Plugin for BillboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbBillboard, Billboard>(
            SceneComponentId::BILLBOARD,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            PostUpdate,
            update_billboards.in_set(PostUpdateSets::Billboard),
        );
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum BillboardMode {
    None,
    Y,
    YX,
    All,
}

impl From<Option<i32>> for BillboardMode {
    fn from(value: Option<i32>) -> Self {
        match value {
            Some(0) => BillboardMode::None,
            Some(2) => BillboardMode::Y,
            Some(3) => BillboardMode::YX,
            _ => BillboardMode::All,
        }
    }
}

#[derive(Component, PartialEq, Eq)]
pub struct Billboard {
    pub mode: BillboardMode,
    // scene entity to face instead of the camera; `None` (unset, or the camera reserved entity)
    // means face the main camera
    pub target: Option<SceneEntityId>,
}

impl From<PbBillboard> for Billboard {
    fn from(value: PbBillboard) -> Self {
        let target = value
            .target_entity
            .map(SceneEntityId::from_proto_u32)
            .filter(|target| *target != SceneEntityId::CAMERA);
        Billboard {
            mode: value.billboard_mode.into(),
            target,
        }
    }
}

pub(crate) fn update_billboards(
    global_transforms: Query<&GlobalTransform>,
    mut q: Query<(
        &mut Transform,
        &GlobalTransform,
        &Billboard,
        &ChildOf,
        &SceneEntity,
    )>,
    contexts: Query<&RendererSceneContext>,
    cam: Query<&Transform, (With<PrimaryCamera>, Without<Billboard>)>,
) {
    // read the camera's local Transform rather than its GlobalTransform: this system runs after
    // CameraUpdate (which writes the camera's local Transform) but before the frame's transform
    // propagation, so the camera's GlobalTransform is still a frame stale. the camera is a root
    // entity, so its GlobalTransform is just its Transform.
    let cam_global_transform = cam.single().ok().map(|t| GlobalTransform::from(*t));

    for (mut local_transform, global_transform, billboard, parent, scene_entity) in q.iter_mut() {
        if billboard.mode == BillboardMode::None {
            continue;
        }

        // resolve the transform to face: the main camera by default, or the target entity if set.
        // if the target can't be resolved (missing / deleted), skip so the entity keeps its rotation.
        let target_global_transform = match billboard.target {
            None => cam_global_transform,
            Some(target) => contexts
                .get(scene_entity.root)
                .ok()
                .and_then(|ctx| ctx.bevy_entity(target))
                .and_then(|entity| global_transforms.get(entity).ok().copied()),
        };
        let Some(target_global_transform) = target_global_transform else {
            continue;
        };
        let (_, target_g_rotation, target_g_translation) =
            target_global_transform.to_scale_rotation_translation();
        let target_z = target_g_rotation.to_euler(EulerRot::YXZ).2;

        // get reference frame
        let frame = global_transforms.get(parent.parent()).unwrap();

        match billboard.mode {
            BillboardMode::None => unreachable!(),
            BillboardMode::All => {
                // use global frame of reference
                let (g_scale, _, g_translation) = global_transform.to_scale_rotation_translation();
                let target_direction = target_g_translation - g_translation;
                let target_global_rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    target_direction.x.atan2(target_direction.z),
                    -target_direction.y.atan2(target_direction.xz().length()),
                    target_z,
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
            BillboardMode::Y | BillboardMode::YX => {
                // map target into local frame
                // TODO use GlobalTransform::raparented_to
                let target_local_matrix =
                    frame.compute_matrix().inverse() * target_global_transform.compute_matrix();
                let (_, _, target_local_translation) =
                    target_local_matrix.to_scale_rotation_translation();

                let target_direction = target_local_translation - local_transform.translation;
                let mut euler_angles = local_transform.rotation.to_euler(EulerRot::YXZ);

                // rotate to face / yaw
                euler_angles.0 = target_direction.x.atan2(target_direction.z);

                if billboard.mode == BillboardMode::YX {
                    // tilt to face / pitch
                    euler_angles.1 = -target_direction.y.atan2(target_direction.xz().length());
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
