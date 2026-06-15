use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    platform::collections::HashMap,
    prelude::*,
    render::mesh::MeshTag,
};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    inputs::{CommonInputAction, SystemAction},
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::{
        PlayerModifiers, PointerTargetType, PrimaryCamera, PrimaryUser, ShowProfileEvent, ToolTips,
        TooltipSource,
    },
    util::AsH160,
};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use dcl_component::{proto_components::sdk::components::ColliderLayer, SceneEntityId};
use input_manager::{InputManager, InputPriority, InputType};
use rapier3d_f64::{
    na::Isometry,
    prelude::{ColliderBuilder, Group, InteractionGroups, SharedShape},
};
use scene_material::{SceneMaterial, SCENE_MATERIAL_OUTLINE_RED_MESH_TAG};
use scene_runner::{
    update_scene::pointer_results::{AvatarColliders, PointerTarget},
    update_world::mesh_collider::ColliderId,
};
use serde_json::json;
use system_bridge::{AvatarModifierState, NativeUi, SystemApi};

pub struct AvatarColliderPlugin;

impl Plugin for AvatarColliderPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AvatarHighlighted>();
        app.init_resource::<PlayerClickedSenders>();

        app.add_plugins(HierarchyPropagatePlugin::<AvatarOutline>::default());

        app.add_systems(
            Update,
            (
                update_avatar_colliders.in_set(SceneSets::PostInit),
                (
                    clean_player_clicked_senders,
                    collect_player_clicked_senders.run_if(on_event::<RpcCall>),
                    update_avatar_collider_actions.in_set(SceneSets::Input),
                    send_message_to_scene.run_if(in_state(AvatarHighlighted(true))),
                )
                    .chain(),
                handle_avatar_modifier_requests,
            ),
        );
        app.add_observer(avatar_outline_on_add);
        app.add_observer(avatar_outline_on_remove);
    }
}

fn update_avatar_colliders(
    mut colliders: ResMut<AvatarColliders>,
    foreign_players: Query<(Entity, &ForeignPlayer, &GlobalTransform)>,
    primary_player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
) {
    let mut positions = foreign_players
        .iter()
        .map(|(e, f, t)| (f.scene_id, (e, t)))
        .collect::<HashMap<_, _>>();
    if let Ok((e, gt)) = primary_player.single() {
        positions.insert(SceneEntityId::PLAYER, (e, gt));
    }

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
                .update_collider_transform(&id, &transform);
        } else {
            // collider didn't exist, make a new one
            let collider = ColliderBuilder::new(SharedShape::capsule_y(
                (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS) as f64,
                PLAYER_COLLIDER_RADIUS as f64,
            ))
            .position(Isometry::from_parts(
                (transform.translation() + PLAYER_COLLIDER_HEIGHT * 0.5 * Vec3::Y)
                    .as_dvec3()
                    .into(),
                Default::default(),
            ))
            .collision_groups(InteractionGroups {
                memberships: Group::from_bits_truncate(ColliderLayer::ClPlayer as u32),
                filter: Group::from_bits_truncate(ColliderLayer::ClPlayer as u32),
                test_mode: rapier3d_f64::prelude::InteractionTestMode::And,
            })
            .build();
            colliders
                .collider_data
                .set_collider(&id, collider, Some(ent));
            colliders.lookup.insert(id, ent);
        }
    }
}

#[derive(Default, Resource, Deref, DerefMut)]
struct PlayerClickedSenders(Vec<RpcEventSender>);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
struct AvatarHighlighted(bool);

#[derive(Default, Clone, Copy, PartialEq, Eq, Component)]
struct AvatarOutline;

fn clean_player_clicked_senders(mut senders: ResMut<PlayerClickedSenders>) {
    senders.retain(|s| !s.is_closed());
}

fn collect_player_clicked_senders(
    mut senders: ResMut<PlayerClickedSenders>,
    mut subscribe_events: EventReader<RpcCall>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerClicked { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }
}

fn update_avatar_collider_actions(
    mut commands: Commands,
    pointer_target: Res<PointerTarget>,
    profiles: Query<&PlayerModifiers, With<ForeignPlayer>>,
    mut previous_target: Local<Option<Entity>>,
) {
    if previous_target.as_ref() != pointer_target.0.as_ref().map(|target| &target.container) {
        debug!(
            "Pointer target changed from {:?} to {:?}",
            previous_target.as_ref(),
            pointer_target.0.as_ref().map(|target| &target.container)
        );
        let maybe_old_target = previous_target.take();
        *previous_target = pointer_target.0.as_ref().map(|target| target.container);

        if let Some(old_target) = maybe_old_target {
            debug!("Reseting outline of {}", old_target);
            commands
                .entity(old_target)
                .try_remove::<Propagate<AvatarOutline>>();
        }

        if let Some(target) = pointer_target.0.as_ref() {
            if target.ty == PointerTargetType::Avatar {
                let Ok(modifiers) = profiles.get(target.container) else {
                    return;
                };

                // check modifier
                if modifiers.hide_profile {
                    return;
                }

                // hilight meshes of target container
                debug!("Highlighting avatar {}", target.container);
                commands.set_state(AvatarHighlighted(true));
                commands
                    .entity(target.container)
                    .try_insert(Propagate(AvatarOutline));
                return;
            }
        }

        commands.set_state(AvatarHighlighted(false));
    }
}

#[expect(clippy::too_many_arguments)]
fn send_message_to_scene(
    mut commands: Commands,
    pointer_target: Res<PointerTarget>,
    profiles: Query<(&ForeignPlayer, &UserProfile)>,
    camera: Single<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    senders: Res<PlayerClickedSenders>,
    mut input_manager: InputManager,
    native_ui: Res<NativeUi>,
    mut tooltips: ResMut<ToolTips>,
) {
    tooltips.0.remove(&TooltipSource::Label("avatar_pointer"));

    if native_ui.profile {
        input_manager.priorities().reserve(
            InputType::Action(SystemAction::ShowProfile.into()),
            InputPriority::AvatarCollider,
        );
    }

    input_manager.priorities().release(
        InputType::Action(SystemAction::ShowProfile.into()),
        InputPriority::AvatarCollider,
    );

    let Some(target) = pointer_target.0.as_ref() else {
        return;
    };

    let Ok((player, profile)) = profiles.get(target.container) else {
        return;
    };

    if input_manager.just_down(CommonInputAction::IaPointer, InputPriority::Scene) {
        let camera_position = camera.1.translation();
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

    if native_ui.profile {
        tooltips.0.insert(
            TooltipSource::Label("avatar_pointer"),
            vec![("Middle Click : Profile".to_owned(), true)],
        );
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

fn avatar_outline_on_add(
    trigger: Trigger<OnAdd, AvatarOutline>,
    mut meshes: Query<(&mut Mesh3d, &mut MeshTag), With<MeshMaterial3d<SceneMaterial>>>,
) {
    let entity = trigger.target();
    let Ok((mut mesh_3d, mut mesh_tag)) = meshes.get_mut(entity) else {
        return;
    };
    mesh_3d.set_changed();
    mesh_tag.0 |= SCENE_MATERIAL_OUTLINE_RED_MESH_TAG;
}

fn avatar_outline_on_remove(
    trigger: Trigger<OnRemove, AvatarOutline>,
    mut meshes: Query<(&mut Mesh3d, &mut MeshTag), With<MeshMaterial3d<SceneMaterial>>>,
) {
    let entity = trigger.target();
    let Ok((mut mesh_3d, mut mesh_tag)) = meshes.get_mut(entity) else {
        return;
    };
    mesh_3d.set_changed();
    mesh_tag.0 &= !SCENE_MATERIAL_OUTLINE_RED_MESH_TAG;
}

fn handle_avatar_modifier_requests(
    mut events: EventReader<SystemApi>,
    players: Query<(&ForeignPlayer, &PlayerModifiers)>,
) {
    for ev in events.read() {
        if let SystemApi::GetAvatarModifiers(sender) = ev {
            let response: Vec<AvatarModifierState> = players
                .iter()
                .filter(|(_, m)| m.hide || m.hide_profile)
                .map(|(fp, m)| AvatarModifierState {
                    user_id: format!("{:#x}", fp.address),
                    hide_avatar: m.hide,
                    hide_profile: m.hide_profile,
                })
                .collect();
            sender.send(response);
        }
    }
}
