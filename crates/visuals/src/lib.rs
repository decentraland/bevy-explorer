use std::f32::consts::FRAC_PI_2;

use bevy::{
    pbr::{wireframe::WireframePlugin, DirectionalLightShadowMap},
    prelude::*,
};
use bevy_atmosphere::{
    prelude::{AtmosphereCamera, AtmosphereModel, AtmospherePlugin, Nishita},
    system_param::AtmosphereMut,
};

use common::{
    sets::SetupSets,
    structs::{PrimaryCameraRes, PrimaryUser},
    util::TryInsertEx,
};

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 4096 })
            .insert_resource(AtmosphereModel::default())
            .add_plugins(AtmospherePlugin)
            .add_plugins(WireframePlugin)
            .add_systems(Update, daylight_cycle)
            .add_systems(Update, move_ground)
            .add_systems(Startup, setup.in_set(SetupSets::Main));
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Res<PrimaryCameraRes>,
) {
    info!("visuals::setup");

    commands
        .entity(camera.0)
        .try_insert(AtmosphereCamera::default())
        .try_insert(FogSettings {
            color: Color::rgb(0.3, 0.2, 0.1),
            directional_light_color: Color::rgb(1.0, 1.0, 0.7),
            directional_light_exponent: 10.0,
            falloff: FogFalloff::ExponentialSquared { density: 0.015 },
        });

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(
                shape::Plane {
                    size: 50000.0,
                    subdivisions: 10,
                }
                .into(),
            ),
            material: materials.add(StandardMaterial {
                base_color: Color::rgb(0.15, 0.2, 0.05),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                depth_bias: -100.0,
                ..Default::default()
            }),
            ..Default::default()
        },
        Ground,
    ));
}

fn daylight_cycle(
    mut fog: Query<&mut FogSettings>,
    mut atmosphere: AtmosphereMut<Nishita>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight)>,
    time: Res<Time>,
) {
    let t = 120.0 + time.elapsed_seconds_wrapped() / 200.0;
    let rotation = Quat::from_euler(EulerRot::YXZ, FRAC_PI_2 * 0.8, -t, 0.0);
    atmosphere.sun_position = rotation * Vec3::Z;

    if let Ok((mut light_trans, mut directional)) = sun.get_single_mut() {
        light_trans.rotation = rotation;
        directional.illuminance = t.sin().max(0.0).powf(2.0) * 30000.0;

        if let Ok(mut fog) = fog.get_single_mut() {
            let sun_up = atmosphere.sun_position.dot(Vec3::Y);
            let rgb = Vec3::new(0.4, 0.4, 0.2) * sun_up.clamp(0.0, 1.0)
                + Vec3::new(0.0, 0.0, 0.0) * (8.0 * (0.125 - sun_up.clamp(0.0, 0.125)));
            fog.color = Color::rgb(rgb.x, rgb.y, rgb.z);
        }
    }
}

#[derive(Component)]
struct Ground;

fn move_ground(
    mut ground: Query<&mut Transform, With<Ground>>,
    cam: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    let Ok(mut transform) = ground.get_single_mut() else {
        return;
    };

    let Ok(target) = cam.get_single() else {
        return;
    };

    transform.translation = target.translation() * Vec3::new(1.0, 0.0, 1.0);
}
