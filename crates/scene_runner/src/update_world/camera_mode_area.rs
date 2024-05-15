use bevy::{prelude::*, utils::HashSet};

use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    sets::SceneSets,
    structs::{CameraOverride, CinematicSettings, PrimaryCamera, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        common::CameraType, CinematicControlType, PbCameraModeArea,
    },
    SceneComponentId, SceneEntityId,
};

use crate::{renderer_context::RendererSceneContext, ContainingScene, SceneEntity, Toaster};

use super::AddCrdtInterfaceExt;

pub struct CameraModeAreaPlugin;

#[derive(Component, Debug)]
pub struct CameraModeArea(pub PbCameraModeArea);

impl From<PbCameraModeArea> for CameraModeArea {
    fn from(value: PbCameraModeArea) -> Self {
        Self(value)
    }
}

impl Plugin for CameraModeAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbCameraModeArea, CameraModeArea>(
            SceneComponentId::CAMERA_MODE_AREA,
            ComponentPosition::Any,
        );

        app.add_systems(Update, update_camera_mode_area.in_set(SceneSets::PostLoop));
    }
}

#[allow(clippy::too_many_arguments)]
fn update_camera_mode_area(
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    areas: Query<(Entity, &SceneEntity, &CameraModeArea, &GlobalTransform)>,
    contexts: Query<&RendererSceneContext>,
    mut current_areas: Local<Vec<Entity>>,
    mut camera: Query<&mut PrimaryCamera>,
    mut toaster: Toaster,
    gt_helper: TransformHelper,
) {
    let Ok(mut camera) = camera.get_single_mut() else {
        return;
    };

    let (scenes, player_position) = player
        .get_single()
        .map(|(player, player_transform)| {
            (
                containing_scene
                    .get_area(player, PLAYER_COLLIDER_RADIUS)
                    .into_iter()
                    .collect::<HashSet<_>>(),
                player_transform.translation(),
            )
        })
        .unwrap_or_default();

    // utility to check if player is within a camera area
    let player_in_area = |area: &CameraModeArea, transform: &GlobalTransform| -> bool {
        let (_, rotation, translation) = transform.to_scale_rotation_translation();
        let player_relative_position = rotation.inverse() * (player_position - translation);
        let area = area.0.area.unwrap_or_default().abs_vec_to_vec3() * 0.5
            + Vec3::new(
                PLAYER_COLLIDER_RADIUS,
                PLAYER_COLLIDER_HEIGHT,
                PLAYER_COLLIDER_RADIUS,
            );

        // check bounds
        player_relative_position.clamp(-area, area) == player_relative_position
    };

    // check areas
    for (ent, scene_ent, area, transform) in areas.iter() {
        let current_index = current_areas
            .iter()
            .enumerate()
            .find(|(_, e)| ent == **e)
            .map(|(ix, _)| ix);
        let in_area = scenes.contains(&scene_ent.root) && player_in_area(area, transform);

        if in_area == current_index.is_some() {
            continue;
        }

        match current_index {
            // remove if no longer in area
            Some(index) => {
                current_areas.remove(index);
            }
            // add at end if newly entered
            None => current_areas.push(ent),
        }
    }

    // remove destroyed areas
    current_areas.retain(|area_ent| areas.get(*area_ent).is_ok());

    // apply last-entered
    match current_areas.last() {
        Some(area_ent) => {
            let (_, scene_ent, area, _) = areas.get(*area_ent).unwrap();

            match area.0.mode() {
                CameraType::CtFirstPerson => {
                    camera.scene_override = Some(CameraOverride::Distance(0.0))
                }
                CameraType::CtThirdPerson => {
                    camera.scene_override = Some(CameraOverride::Distance(1.0))
                }
                CameraType::CtCinematic => {
                    let Some(target_entity) =
                        area.0.cinematic_entity.map(SceneEntityId::from_proto_u32)
                    else {
                        warn!("no target entity");
                        return;
                    };
                    let Ok(ctx) = contexts.get(scene_ent.root) else {
                        warn!("no scene");
                        return;
                    };
                    let Some(cam) = ctx.bevy_entity(target_entity) else {
                        warn!("no scene cam");
                        return;
                    };
                    let Ok(origin) = gt_helper.compute_global_transform(cam) else {
                        warn!("failed to get gt");
                        return;
                    };
                    camera.scene_override = Some(CameraOverride::Cinematic(CinematicSettings {
                        origin,
                        avatar_control: match area.0.cinematic_avatar_control() {
                            CinematicControlType::CctNone => {
                                common::structs::CinematicControl::None
                            }
                            CinematicControlType::CctRelative => {
                                common::structs::CinematicControl::Relative
                            }
                            CinematicControlType::CctTank => {
                                common::structs::CinematicControl::Tank
                            }
                        },
                        camera_control: area.0.cinematic_camera_control(),
                    }));
                }
            }
            toaster.add_toast("camera_mode_area", "The scene has enforced the camera view");
        }
        None => {
            // no camera areas
            camera.scene_override = None;
            toaster.clear_toast("camera_mode_area");
        }
    }
}
