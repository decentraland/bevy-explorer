use bevy::{prelude::*, utils::HashSet};

use crate::{
    permissions::Permission, renderer_context::RendererSceneContext, ContainingScene, SceneEntity,
};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    sets::SceneSets,
    structs::{CameraOverride, CinematicSettings, PermissionType, PrimaryCamera, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        common::CameraType, PbCameraModeArea, PbMainCamera, PbVirtualCamera,
    },
    SceneComponentId, SceneEntityId,
};

use super::AddCrdtInterfaceExt;

pub struct CameraModeAreaPlugin;

#[derive(Component, Debug)]
pub struct CameraModeArea(pub PbCameraModeArea);

impl From<PbCameraModeArea> for CameraModeArea {
    fn from(value: PbCameraModeArea) -> Self {
        Self(value)
    }
}

#[derive(Component, Debug)]
pub struct VirtualCamera(pub PbVirtualCamera);

impl From<PbVirtualCamera> for VirtualCamera {
    fn from(value: PbVirtualCamera) -> Self {
        Self(value)
    }
}

#[derive(Component, Debug)]
pub struct MainSceneCamera(pub PbMainCamera);

impl From<PbMainCamera> for MainSceneCamera {
    fn from(value: PbMainCamera) -> Self {
        Self(value)
    }
}

impl Plugin for CameraModeAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbCameraModeArea, CameraModeArea>(
            SceneComponentId::CAMERA_MODE_AREA,
            ComponentPosition::Any,
        );

        app.add_crdt_lww_component::<PbVirtualCamera, VirtualCamera>(
            SceneComponentId::VIRTUAL_CAMERA,
            ComponentPosition::EntityOnly,
        );

        app.add_crdt_lww_component::<PbMainCamera, MainSceneCamera>(
            SceneComponentId::MAIN_CAMERA,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, update_camera_mode_area.in_set(SceneSets::PostLoop));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PermissionState {
    Resolved(bool),
    NotRequested,
    Pending,
}

#[allow(clippy::too_many_arguments)]
pub fn update_camera_mode_area(
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    areas: Query<
        (
            Entity,
            &SceneEntity,
            Option<&CameraModeArea>,
            &GlobalTransform,
            Option<&VirtualCamera>,
        ),
        Or<(With<CameraModeArea>, With<VirtualCamera>)>,
    >,
    virtual_cameras: Query<&VirtualCamera>,
    main_scene_cameras: Query<&MainSceneCamera>,
    contexts: Query<&RendererSceneContext>,
    mut current_areas: Local<Vec<(Entity, PermissionState)>>,
    mut camera: Query<&mut PrimaryCamera>,
    mut perms: Permission<Entity>,
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
            ) * if area.0.use_collider_range.unwrap_or(true) {
                1.0
            } else {
                0.0
            };

        // check bounds
        player_relative_position.clamp(-area, area) == player_relative_position
    };

    // check areas
    for (ent, scene_ent, maybe_area, transform, _) in areas.iter() {
        let current_index = current_areas
            .iter()
            .enumerate()
            .find(|(_, (e, _))| ent == *e)
            .map(|(ix, _)| ix);
        let in_area = scenes.contains(&scene_ent.root)
            && maybe_area.is_some_and(|area| player_in_area(area, transform));

        if in_area == current_index.is_some() {
            continue;
        }

        match current_index {
            // remove if no longer in area
            Some(index) => {
                current_areas.remove(index);
            }
            // add at end if newly entered
            None => current_areas.push((ent, PermissionState::NotRequested)),
        }
    }

    // remove destroyed areas
    current_areas.retain(|(area_ent, _)| areas.get(*area_ent).is_ok());

    // apply last-entered
    let area = current_areas
        .iter_mut()
        .rev()
        .filter_map(|(ent, permitted)| match permitted {
            PermissionState::Resolved(true) => Some(*ent),
            PermissionState::NotRequested => {
                let (_, scene_ent, _, _, _) = areas.get(*ent).unwrap();
                perms.check_unique(
                    PermissionType::ForceCamera,
                    scene_ent.root,
                    *ent,
                    None,
                    false,
                );
                *permitted = PermissionState::Pending;
                None
            }
            _ => None,
        })
        .next();

    let virtual_camera = scenes
        .iter()
        .flat_map(|scene| {
            let Ok(ctx) = contexts.get(*scene) else {
                return None;
            };

            ctx.bevy_entity(SceneEntityId::CAMERA)
                .and_then(|cam_ent| main_scene_cameras.get(cam_ent).ok())
                .and_then(|main_cam| main_cam.0.virtual_camera_entity)
                .and_then(|virtual_scene_ent| {
                    ctx.bevy_entity(SceneEntityId::from_proto_u32(virtual_scene_ent))
                })
                .filter(|virtual_bevy_ent| virtual_cameras.get(*virtual_bevy_ent).is_ok())
        })
        .next();

    // use virtual cam if specified, fall back to area
    let area_or_virtual = virtual_camera.or(area);

    if let Some(area_or_virtual) = area_or_virtual {
        let (bevy_ent, scene_ent, maybe_area, _, maybe_virtual) =
            areas.get(area_or_virtual).unwrap();

        match maybe_area.map(|area| area.0.mode()) {
            Some(CameraType::CtFirstPerson) => {
                camera.scene_override = Some(CameraOverride::Distance(0.0))
            }
            Some(CameraType::CtThirdPerson) => {
                camera.scene_override = Some(CameraOverride::Distance(1.0))
            }
            Some(CameraType::CtCinematic) => {
                let Some(cinematic_settings) =
                    maybe_area.as_ref().unwrap().0.cinematic_settings.as_ref()
                else {
                    warn!("no cinematic settings");
                    return;
                };
                let target_entity = SceneEntityId::from_proto_u32(cinematic_settings.camera_entity);
                let Ok(ctx) = contexts.get(scene_ent.root) else {
                    warn!("no scene");
                    return;
                };
                let Some(cam) = ctx.bevy_entity(target_entity) else {
                    warn!("no scene cam");
                    return;
                };
                camera.scene_override = Some(CameraOverride::Cinematic(CinematicSettings {
                    origin: cam,
                    allow_manual_rotation: cinematic_settings
                        .allow_manual_rotation
                        .unwrap_or_default(),
                    yaw_range: cinematic_settings.yaw_range,
                    pitch_range: cinematic_settings.pitch_range,
                    roll_range: cinematic_settings.roll_range,
                    zoom_min: cinematic_settings.zoom_min,
                    zoom_max: cinematic_settings.zoom_max,
                    look_at_entity: None,
                    transition: None,
                }));
            }
            None => {
                let Ok(ctx) = contexts.get(scene_ent.root) else {
                    warn!("no scene");
                    return;
                };
                camera.scene_override = Some(CameraOverride::Cinematic(CinematicSettings {
                    origin: bevy_ent,
                    allow_manual_rotation: false,
                    yaw_range: Some(0.0),
                    pitch_range: Some(0.0),
                    roll_range: Some(0.0),
                    zoom_min: None,
                    zoom_max: None,
                    look_at_entity: maybe_virtual
                        .as_ref()
                        .and_then(|v| v.0.look_at_entity)
                        .and_then(|look_at| {
                            ctx.bevy_entity(SceneEntityId::from_proto_u32(look_at))
                        }),
                    transition: maybe_virtual
                        .as_ref()
                        .and_then(|v| v.0.default_transition.clone()),
                }));
            }
        }
    } else {
        // no camera areas
        camera.scene_override = None;
    }

    if current_areas.is_empty() {
        perms
            .toaster
            .clear_toast(format!("{:?}", PermissionType::ForceCamera).as_str());
    }

    let succeeded = perms
        .drain_success(PermissionType::ForceCamera)
        .collect::<HashSet<_>>();
    let failed = perms
        .drain_fail(PermissionType::ForceCamera)
        .collect::<HashSet<_>>();

    for (area, state) in current_areas.iter_mut() {
        if succeeded.contains(area) {
            *state = PermissionState::Resolved(true);
        }
        if failed.contains(area) {
            *state = PermissionState::Resolved(false);
        }
    }
}
