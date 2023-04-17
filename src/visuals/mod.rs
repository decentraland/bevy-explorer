use std::{f32::consts::FRAC_PI_2, time::Duration};

use bevy::{pbr::DirectionalLightShadowMap, prelude::*};
use bevy_atmosphere::{
    prelude::{AtmosphereCamera, AtmosphereModel, AtmospherePlugin, Nishita},
    system_param::AtmosphereMut,
};

use crate::scene_runner::PrimaryCamera;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 4096 })
            .insert_resource(AtmosphereModel::default())
            .add_plugin(AtmospherePlugin)
            .add_system(daylight_cycle)
            .add_system(setup);
    }
}

fn setup(
    mut commands: Commands,
    camera: Query<Entity, (With<PrimaryCamera>, Without<AtmosphereCamera>)>,
) {
    if let Ok(cam_entity) = camera.get_single() {
        commands
            .entity(cam_entity)
            .insert(AtmosphereCamera::default());
    }
}

fn daylight_cycle(
    mut atmosphere: AtmosphereMut<Nishita>,
    mut query: Query<(&mut Transform, &mut DirectionalLight)>,
    mut timer: Local<Timer>,
    time: Res<Time>,
) {
    if timer.mode() == TimerMode::Once {
        *timer = Timer::new(Duration::from_millis(50), TimerMode::Repeating);
    }
    timer.tick(time.delta());

    if timer.finished() {
        let t = 0.1 + time.elapsed_seconds_wrapped() / 200.0;
        let rotation = Quat::from_euler(EulerRot::YXZ, FRAC_PI_2 * 0.8, -t, 0.0);
        atmosphere.sun_position = rotation * Vec3::Z;

        if let Some((mut light_trans, mut directional)) = query.single_mut().into() {
            light_trans.rotation = rotation;
            directional.illuminance = t.sin().max(0.0).powf(2.0) * 1000000.0;
        }
    }
}
