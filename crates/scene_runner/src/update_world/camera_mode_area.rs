use bevy::{prelude::*, utils::HashSet};

use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    sets::SceneSets,
    structs::{CameraOverride, PrimaryCamera, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{common::CameraType, PbCameraModeArea},
    SceneComponentId,
};

use crate::{ContainingScene, SceneEntity, Toaster};

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

        app.add_system(update_camera_mode_area.in_set(SceneSets::PostLoop));
    }
}

fn update_camera_mode_area(
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    areas: Query<(Entity, &SceneEntity, &CameraModeArea, &GlobalTransform)>,
    mut current_areas: Local<Vec<Entity>>,
    mut camera: Query<&mut PrimaryCamera>,
    mut toaster: Toaster,
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

    // check any new areas
    for (ent, scene_ent, area, transform) in areas.iter() {
        // scene is active, area not already active, player in the area
        // TODO check scene perms
        if scenes.contains(&scene_ent.root)
            && !current_areas.iter().any(|e| ent == *e)
            && player_in_area(area, transform)
        {
            current_areas.push(ent);
        }
    }

    // check existing areas, removing invalids
    while let Some(area_ent) = current_areas.pop() {
        if let Ok((_, scene_ent, area, transform)) = areas.get(area_ent) {
            if scenes.contains(&scene_ent.root) && player_in_area(area, transform) {
                match area.0.mode() {
                    CameraType::CtFirstPerson => {
                        camera.scene_override = Some(CameraOverride::Distance(0.0))
                    }
                    CameraType::CtThirdPerson => {
                        camera.scene_override = Some(CameraOverride::Distance(1.0))
                    }
                    CameraType::CtCinematic => {
                        warn!("cinematic camera not supported");
                        camera.scene_override = None;
                    }
                }
                toaster.add_toast("camera_mode_area", "The scene has enforced the camera view");
                current_areas.push(area_ent);
                return;
            }
        }
    }

    // no camera areas
    camera.scene_override = None;
    toaster.clear_toast("camera_mode_area");
}
