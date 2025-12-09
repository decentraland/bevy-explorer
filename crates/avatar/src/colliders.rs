use crate::AvatarMaterials;
use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS},
    inputs::{CommonInputAction, SystemAction},
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::{PlayerModifiers, PrimaryCamera, ShowProfileEvent, ToolTips, TooltipSource},
    util::AsH160,
};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use input_manager::{InputManager, InputPriority, InputType};
use rapier3d::{
    na::Isometry,
    prelude::{ColliderBuilder, SharedShape},
};
use scene_material::{SceneMaterial, SCENE_MATERIAL_OUTLINE_RED};
use scene_runner::{
    update_scene::pointer_results::{AvatarColliders, PointerTarget, PointerTargetType},
    update_world::mesh_collider::ColliderId,
};
use serde_json::json;
use system_bridge::NativeUi;

pub struct AvatarColliderPlugin;

impl Plugin for AvatarColliderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_avatar_colliders.in_set(SceneSets::PostInit),
                update_avatar_collider_actions.in_set(SceneSets::Input),
            ),
        );
    }
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
                PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS,
                PLAYER_COLLIDER_RADIUS - PLAYER_COLLIDER_OVERLAP,
            ))
            .position(Isometry::from_parts(
                (transform.translation() + PLAYER_COLLIDER_HEIGHT * 0.5 * Vec3::Y).into(),
                Default::default(),
            ))
            .build();
            colliders
                .collider_data
                .set_collider(&id, collider, Some(ent));
            colliders.lookup.insert(id, ent);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_avatar_collider_actions(
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    pointer_target: Res<PointerTarget>,
    mut tooltips: ResMut<ToolTips>,
    profiles: Query<(
        &ForeignPlayer,
        &UserProfile,
        &PlayerModifiers,
        Ref<AvatarMaterials>,
    )>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
    mut hilighted_materials: Local<HashSet<AssetId<SceneMaterial>>>,
    mut scene_materials: ResMut<Assets<SceneMaterial>>,
    mut input_manager: InputManager,
    native_ui: Res<NativeUi>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerClicked { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    tooltips.0.remove(&TooltipSource::Label("avatar_pointer"));

    // reset old mats
    for mat in hilighted_materials.drain() {
        if let Some(mat) = scene_materials.get_mut(mat) {
            mat.extension.data.flags &= !SCENE_MATERIAL_OUTLINE_RED;
        }
    }

    input_manager.priorities().release(
        InputType::Action(SystemAction::ShowProfile.into()),
        InputPriority::AvatarCollider,
    );

    if let Some(target) = pointer_target.0.as_ref() {
        if target.ty == PointerTargetType::Avatar {
            if native_ui.profile {
                input_manager.priorities().reserve(
                    InputType::Action(SystemAction::ShowProfile.into()),
                    InputPriority::AvatarCollider,
                );
            }

            let Ok((player, profile, modifiers, materials)) = profiles.get(target.container) else {
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

            if native_ui.profile {
                tooltips.0.insert(
                    TooltipSource::Label("avatar_pointer"),
                    vec![("Middle Click : Profile".to_owned(), true)],
                );
            }

            if input_manager.just_down(CommonInputAction::IaPointer, InputPriority::Scene) {
                let camera_position = camera
                    .single()
                    .map(|(_, gt)| gt.translation())
                    .unwrap_or_default();
                let direction = (target.position.unwrap() - camera_position).normalize();

                // send event
                let event = json!({
                "userId": format!("{:#x}", player.address),
                "ray": {
                    "origin": { "x": camera_position.x, "y": camera_position.y, "z": -camera_position.z },
                    "direction": { "x": direction.x, "y": direction.y, "z": -direction.z },
                    "distance": target.distance.0
                }
            }).to_string();
                for sender in senders.iter() {
                    let _ = sender.send(event.clone());
                }
            }

            if native_ui.profile
                && input_manager.just_down(SystemAction::ShowProfile, InputPriority::AvatarCollider)
            {
                // display profile
                if let Some(address) = profile.content.eth_address.as_h160() {
                    commands.send_event(ShowProfileEvent(address));
                } else {
                    warn!("Profile has a bad address {}", profile.content.eth_address);
                }
            }
        }
    }

    senders.retain(|s| !s.is_closed());
}
