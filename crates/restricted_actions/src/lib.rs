use avatar::AvatarDynamicState;
use bevy::{math::Vec3Swizzles, prelude::*};
use common::{
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser, RestrictedAction},
};
use scene_runner::{initialize_scene::PARCEL_SIZE, renderer_context::RendererSceneContext};
use ui_core::dialog::SpawnDialog;

pub struct RestrictedActionsPlugin;

impl Plugin for RestrictedActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RestrictedAction>();
        app.add_systems(
            Update,
            (move_player, move_camera).in_set(SceneSets::PostLoop),
        );
    }
}

fn move_player(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    scenes: Query<&RendererSceneContext>,
    mut player: Query<(&mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
) {
    for (root, transform) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::MovePlayer { scene, to } => Some((scene, to)),
        _ => None,
    }) {
        let Ok(scene) = scenes.get(*root) else {
            continue;
        };

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
            let (mut player_transform, mut dynamics) = player.single_mut();
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
