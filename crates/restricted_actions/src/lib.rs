use avatar::AvatarDynamicState;
use bevy::{math::Vec3Swizzles, prelude::*};
use common::{
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser, RestrictedAction},
};
use scene_runner::{initialize_scene::PARCEL_SIZE, renderer_context::RendererSceneContext};

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

        let (mut player_transform, mut dynamics) = player.single_mut();
        dynamics.velocity =
            transform.rotation * player_transform.rotation.inverse() * dynamics.velocity;

        *player_transform = *transform;
        player_transform.translation +=
            (scene.base * IVec2::new(1, -1)).as_vec2().extend(0.0).xzy() * PARCEL_SIZE;
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