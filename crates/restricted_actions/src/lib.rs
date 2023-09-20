use avatar::AvatarDynamicState;
use bevy::{math::Vec3Swizzles, prelude::*};
use common::{
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser, RestrictedAction},
};
use ipfs::ChangeRealmEvent;
use scene_runner::{
    initialize_scene::PARCEL_SIZE, renderer_context::RendererSceneContext, ContainingScene,
};
use ui_core::dialog::SpawnDialog;

pub struct RestrictedActionsPlugin;

impl Plugin for RestrictedActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RestrictedAction>();
        app.add_systems(
            Update,
            (move_player, move_camera, change_realm).in_set(SceneSets::PostLoop),
        );
    }
}

fn move_player(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    scenes: Query<&RendererSceneContext>,
    mut player: Query<(Entity, &mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    for (root, transform) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::MovePlayer { scene, to } => Some((scene, to)),
        _ => None,
    }) {
        let Ok(scene) = scenes.get(*root) else {
            continue;
        };

        if player
            .get_single()
            .ok()
            .and_then(|(e, ..)| containing_scene.get(e))
            != Some(*root)
        {
            warn!("invalid move request from non-containing scene");
            return;
        }

        let mut target_transform = *transform;
        target_transform.translation +=
            (scene.base * IVec2::new(1, -1)).as_vec2().extend(0.0).xzy() * PARCEL_SIZE;

        if transform.translation.clamp(
            Vec3::new(0.0, f32::MIN, -PARCEL_SIZE),
            Vec3::new(PARCEL_SIZE, f32::MAX, 0.0),
        ) != transform.translation
        {
            commands.spawn_dialog_two(
                "Teleport".into(),
                "The scene wants to teleport you to another location".into(),
                "Let's go!",
                move |mut player: Query<&mut Transform, With<PrimaryUser>>| {
                    *player.single_mut() = target_transform;
                },
                "No thanks",
                || {},
            );
        } else {
            let (_, mut player_transform, mut dynamics) = player.single_mut();
            dynamics.velocity =
                transform.rotation * player_transform.rotation.inverse() * dynamics.velocity;

            *player_transform = target_transform;
        }
    }
}

fn move_camera(mut events: EventReader<RestrictedAction>, mut camera: Query<&mut PrimaryCamera>) {
    for rotation in events.iter().filter_map(|ev| match ev {
        RestrictedAction::MoveCamera(rotation) => Some(rotation),
        _ => None,
    }) {
        let (yaw, pitch, roll) = rotation.to_euler(EulerRot::YXZ);

        let mut camera = camera.single_mut();
        camera.yaw = yaw;
        camera.pitch = pitch;
        camera.roll = roll;
    }
}

fn change_realm(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    for (scene, to, message, response) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::ChangeRealm {
            scene,
            to,
            message,
            response,
        } => Some((scene, to, message, response)),
        _ => None,
    }) {
        if player
            .get_single()
            .ok()
            .and_then(|e| containing_scene.get(e))
            != Some(*scene)
        {
            warn!("invalid changeRealm request from non-containing scene");
            return;
        }

        let new_realm = to.clone();
        let response_ok = response.clone();
        let response_fail = response.clone();

        commands.spawn_dialog_two(
            "Change Realm".into(),
            format!(
                "The scene wants to move you to a new realm\n`{}`\n{}",
                to.clone(),
                if let Some(message) = message {
                    message
                } else {
                    ""
                }
            ),
            "Let's go!",
            move |mut writer: EventWriter<ChangeRealmEvent>| {
                writer.send(ChangeRealmEvent {
                    new_realm: new_realm.clone(),
                });
                response_ok.send(Ok(String::default()));
            },
            "No thanks",
            move || {
                response_fail.send(Err(String::default()));
            },
        );
    }
}
