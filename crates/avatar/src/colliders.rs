use bevy::{
    core::FrameCount,
    prelude::*,
    render::render_resource::Extent3d,
    utils::{HashMap, HashSet},
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS},
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::{ActiveDialog, PrimaryCamera, ToolTips, PROFILE_UI_RENDERLAYER},
};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use input_manager::AcceptInput;
use rapier3d_f64::{
    na::Isometry,
    prelude::{ColliderBuilder, SharedShape},
};
use scene_material::{SceneMaterial, SCENE_MATERIAL_OUTLINE_RED};
use scene_runner::{
    update_scene::pointer_results::{PointerTarget, UiPointerTarget},
    update_world::{
        avatar_modifier_area::PlayerModifiers,
        mesh_collider::{ColliderId, SceneColliderData},
    },
};
use serde_json::json;
use ui_core::button::DuiButton;

use crate::{avatar_texture::PhotoBooth, AvatarMaterials, AvatarShape};

pub struct AvatarColliderPlugin;

impl Plugin for AvatarColliderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AvatarColliders>();
        app.add_systems(
            Update,
            (
                update_avatar_colliders.in_set(SceneSets::PostInit),
                update_avatar_collider_actions.in_set(SceneSets::Input),
            ),
        );
    }
}

#[derive(Resource, Default)]
pub struct AvatarColliders {
    pub collider_data: SceneColliderData,
    pub lookup: HashMap<ColliderId, Entity>,
}

fn update_avatar_colliders(
    mut colliders: ResMut<AvatarColliders>,
    foreign_players: Query<(Entity, &ForeignPlayer, &GlobalTransform)>,
) {
    let positions = foreign_players
        .iter()
        .map(|(e, f, t)| (f.scene_id, (e, t)))
        .collect::<HashMap<_, _>>();

    let remove = colliders
        .collider_data
        .iter()
        .filter(|id| !positions.contains_key(&id.entity))
        .cloned()
        .collect::<Vec<_>>();
    for id in remove {
        colliders.collider_data.remove_collider(&id);
        colliders.lookup.remove(&id);
    }

    for (id, (ent, transform)) in positions {
        let id = ColliderId {
            entity: id,
            name: None,
            index: 0,
        };
        if colliders.lookup.contains_key(&id) {
            let transform = transform.mul_transform(Transform::from_translation(
                PLAYER_COLLIDER_HEIGHT * 0.5 * Vec3::Y,
            ));
            colliders
                .collider_data
                .update_collider_transform(&id, &transform, None);
        } else {
            // collider didn't exist, make a new one
            let collider = ColliderBuilder::new(SharedShape::capsule_y(
                (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS) as f64,
                (PLAYER_COLLIDER_RADIUS - PLAYER_COLLIDER_OVERLAP) as f64,
            ))
            .position(Isometry::from_parts(
                (transform.translation() + PLAYER_COLLIDER_HEIGHT * 0.5 * Vec3::Y)
                    .as_dvec3()
                    .into(),
                Default::default(),
            ))
            .build();
            colliders.collider_data.set_collider(&id, collider, ent);
            colliders.lookup.insert(id, ent);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_avatar_collider_actions(
    mut commands: Commands,
    ui_target: Res<UiPointerTarget>,
    mut colliders: ResMut<AvatarColliders>,
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    windows: Query<&Window>,
    (accept_input, pointer_target, frame, active_dialog): (
        Res<AcceptInput>,
        Res<PointerTarget>,
        Res<FrameCount>,
        Res<ActiveDialog>,
    ),
    mut tooltips: ResMut<ToolTips>,
    profiles: Query<(
        &ForeignPlayer,
        &UserProfile,
        &PlayerModifiers,
        Ref<AvatarMaterials>,
    )>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
    mut photo_booth: PhotoBooth,
    dui: Res<DuiRegistry>,
    mut hilighted_materials: Local<HashSet<AssetId<SceneMaterial>>>,
    mut scene_materials: ResMut<Assets<SceneMaterial>>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerClicked { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    tooltips.0.remove("avatar_pointer");

    // check for scene ui
    if !matches!(*ui_target, UiPointerTarget::None) {
        return;
    }

    let Ok((camera, camera_position)) = camera.get_single() else {
        // can't do much without a camera
        return;
    };

    // check for system ui
    if !accept_input.mouse {
        return;
    }

    // get new 3d hover target
    let Ok(window) = windows.get_single() else {
        return;
    };
    let cursor_position = if window.cursor.grab_mode == bevy::window::CursorGrabMode::Locked {
        // if pointer locked, just middle
        Vec2::new(window.width(), window.height()) / 2.0
    } else {
        let Some(cursor_position) = window.cursor_position() else {
            // outside window
            return;
        };
        cursor_position
    };

    let Some(ray) = camera.viewport_to_world(camera_position, cursor_position) else {
        error!("no ray, not sure why that would happen");
        return;
    };

    let camera_translation = camera_position.translation();
    let pointer_distance = pointer_target
        .0
        .as_ref()
        .map(|info| (info.position.unwrap_or(camera_translation) - camera_translation).length())
        .unwrap_or(f32::MAX);

    // reset old mats
    for mat in hilighted_materials.drain() {
        if let Some(mat) = scene_materials.get_mut(mat) {
            mat.extension.data.flags &= !SCENE_MATERIAL_OUTLINE_RED;
        }
    }

    if let Some(avatar_target) = colliders.collider_data.cast_ray_nearest(
        frame.0,
        ray.origin,
        ray.direction.into(),
        pointer_distance,
        u32::MAX,
        true,
    ) {
        let avatar = colliders.lookup.get(&avatar_target.id).unwrap();
        let Ok((player, profile, modifiers, materials)) = profiles.get(*avatar) else {
            return;
        };

        // check modifier
        if modifiers.hide_profile {
            return;
        }

        // hilight selected mats
        if materials.0 != *hilighted_materials {
            for id in materials.0.iter() {
                if let Some(mat) = scene_materials.get_mut(*id) {
                    mat.extension.data.flags |= SCENE_MATERIAL_OUTLINE_RED;
                    hilighted_materials.insert(*id);
                }
            }
        }

        tooltips.0.insert(
            "avatar_pointer",
            vec![("Middle Click : Profile".to_owned(), true)],
        );

        if mouse_input.just_pressed(MouseButton::Middle) {
            // send event
            let event = json!({
                "userId": format!("{:#x}", player.address),
                "ray": {
                    "origin": { "x": ray.origin.x, "y": ray.origin.y, "z": -ray.origin.z },
                    "direction": { "x": ray.direction.x, "y": ray.direction.y, "z": -ray.direction.z },
                    "distance": avatar_target.toi
                }
            }).to_string();
            for sender in senders.iter() {
                let _ = sender.send(event.clone());
            }

            // display profile
            let instance = photo_booth.spawn_booth(
                PROFILE_UI_RENDERLAYER,
                AvatarShape::from(profile),
                Extent3d::default(),
                false,
            );

            let Some(permit) = active_dialog.try_acquire() else {
                warn!("can't open profile with other active dialog");
                return;
            };

            let components = commands
                .spawn_template(
                    &dui,
                    "foreign-profile",
                    DuiProps::new()
                        .with_prop("title", format!("{} profile", profile.content.name))
                        .with_prop("booth-instance", instance)
                        .with_prop("eth-address", profile.content.eth_address.clone())
                        .with_prop("buttons", vec![DuiButton::close_happy("Ok")]),
                )
                .unwrap();

            commands.entity(components.root).insert(permit);
        }
    }

    senders.retain(|s| !s.is_closed());
}
