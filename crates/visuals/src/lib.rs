use bevy::{
    core_pipeline::Skybox,
    pbr::{wireframe::WireframePlugin, CascadeShadowConfigBuilder, DirectionalLightShadowMap},
    prelude::*,
    render::{
        render_asset::RenderAssetBytesPerFrame,
        view::{Layer, RenderLayers},
    },
};
use bevy_atmosphere::{
    prelude::{AtmosphereCamera, AtmosphereModel, AtmospherePlugin, Nishita},
    system_param::AtmosphereMut,
};

use bevy_console::ConsoleCommand;
use common::{
    sets::SetupSets,
    structs::{
        AppConfig, FogSetting, PrimaryCamera, PrimaryCameraRes, PrimaryUser, SceneLoadDistance,
        ShadowSetting, GROUND_RENDERLAYER, PRIMARY_AVATAR_LIGHT_LAYER,
    },
};
use console::DoAddConsoleCommand;

pub struct VisualsPlugin {
    pub no_fog: bool,
}

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 4096 })
            .init_resource::<SceneGlobalLight>()
            .insert_resource(AtmosphereModel::default())
            .add_plugins(AtmospherePlugin)
            .add_plugins(WireframePlugin)
            .add_systems(Update, apply_global_light)
            .add_systems(Update, move_ground)
            .add_systems(Startup, setup.in_set(SetupSets::Main))
            .insert_resource(RenderAssetBytesPerFrame::new(16777216));

        app.add_console_command::<ShadowConsoleCommand, _>(shadow_console_command);
        app.add_console_command::<FogConsoleCommand, _>(fog_console_command);
    }
}

#[derive(Component)]
struct DirectionalLightLayer(Layer);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Res<PrimaryCameraRes>,
) {
    info!("visuals::setup");

    commands
        .entity(camera.0)
        .try_insert(AtmosphereCamera::default());

    commands.entity(camera.0).try_insert(FogSettings {
        color: Color::srgb(0.3, 0.2, 0.1),
        directional_light_color: Color::srgb(1.0, 1.0, 0.7),
        directional_light_exponent: 10.0,
        falloff: FogFalloff::ExponentialSquared { density: 0.01 },
    });

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Plane3d::default().mesh().size(50000.0, 50000.0)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.45, 0.2),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                depth_bias: -100.0,
                ..Default::default()
            }),
            ..Default::default()
        },
        Ground,
        GROUND_RENDERLAYER.clone(),
    ));
}

#[derive(Resource, Default, Clone, Debug)]
pub struct SceneGlobalLight {
    pub source: Option<Entity>,
    pub dir_color: Color,
    pub dir_illuminance: f32,
    pub dir_direction: Vec3,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    pub layers: RenderLayers,
}

static TRANSITION_TIME: f32 = 1.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_global_light(
    mut commands: Commands,
    setting: Res<AppConfig>,
    mut atmosphere: AtmosphereMut<Nishita>,
    mut sun: Query<(
        Entity,
        &DirectionalLightLayer,
        &mut Transform,
        &mut DirectionalLight,
    )>,
    mut ambient: ResMut<AmbientLight>,
    time: Res<Time>,
    mut cameras: Query<
        (
            Option<&PrimaryCamera>,
            Option<&mut Skybox>,
            Option<&mut FogSettings>,
        ),
        With<Camera3d>,
    >,
    scene_distance: Res<SceneLoadDistance>,
    scene_global_light: Res<SceneGlobalLight>,
    mut prev: Local<(f32, SceneGlobalLight)>,
    config: Res<AppConfig>,
) {
    let next_light = if prev.0 >= TRANSITION_TIME && prev.1.source == scene_global_light.source {
        scene_global_light.clone()
    } else {
        // transition part way
        let new_amount = if prev.1.source == scene_global_light.source {
            (time.delta_seconds() / (TRANSITION_TIME - prev.0)).clamp(0.0, 1.0)
        } else {
            time.delta_seconds() / TRANSITION_TIME
        };
        let old_amount = 1.0 - new_amount;
        SceneGlobalLight {
            source: scene_global_light.source,
            dir_color: (scene_global_light.dir_color.to_srgba() * new_amount
                + prev.1.dir_color.to_srgba() * old_amount)
                .into(),
            dir_illuminance: scene_global_light.dir_illuminance * new_amount
                + prev.1.dir_illuminance * old_amount,
            dir_direction: prev
                .1
                .dir_direction
                .lerp(scene_global_light.dir_direction, new_amount),
            ambient_color: (scene_global_light.ambient_color.to_srgba() * new_amount
                + prev.1.ambient_color.to_srgba() * old_amount)
                .into(),
            ambient_brightness: scene_global_light.ambient_brightness * new_amount
                + prev.1.ambient_brightness * old_amount,
            layers: scene_global_light.layers.clone(),
        }
    };

    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, next_light.dir_direction);
    atmosphere.sun_position = -next_light.dir_direction;

    let mut directional_layers = RenderLayers::none();
    for (entity, layer, mut light_trans, mut directional) in sun.iter_mut() {
        if !next_light.layers.intersects(&RenderLayers::layer(layer.0)) {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        directional_layers = directional_layers.with(layer.0);
        light_trans.rotation = rotation;
        directional.illuminance = next_light.dir_illuminance;
        directional.color = next_light.dir_color;
    }

    for new_layer in next_light
        .layers
        .symmetric_difference(&directional_layers)
        .iter()
    {
        let mut layer = RenderLayers::layer(new_layer);
        if new_layer == 0 {
            layer = layer.union(&PRIMARY_AVATAR_LIGHT_LAYER);
        }

        let (shadows_enabled, cascade_shadow_config) = match config.graphics.shadow_settings {
            ShadowSetting::Off => (false, Default::default()),
            ShadowSetting::Low => (
                true,
                CascadeShadowConfigBuilder {
                    num_cascades: 1,
                    minimum_distance: 0.1,
                    maximum_distance: config.graphics.shadow_distance,
                    first_cascade_far_bound: config.graphics.shadow_distance,
                    overlap_proportion: 0.2,
                }
                .build(),
            ),
            ShadowSetting::High => (
                true,
                CascadeShadowConfigBuilder {
                    num_cascades: 4,
                    minimum_distance: 0.1,
                    maximum_distance: config.graphics.shadow_distance,
                    first_cascade_far_bound: config.graphics.shadow_distance / 15.0,
                    overlap_proportion: 0.2,
                }
                .build(),
            ),
        };

        commands.spawn((
            DirectionalLightBundle {
                directional_light: DirectionalLight {
                    color: next_light.dir_color,
                    illuminance: next_light.dir_illuminance,
                    shadows_enabled,
                    ..Default::default()
                },
                transform: Transform::default().with_rotation(rotation),
                cascade_shadow_config,
                ..Default::default()
            },
            layer,
            DirectionalLightLayer(new_layer),
        ));
    }

    for (maybe_primary, maybe_skybox, maybe_fog) in cameras.iter_mut() {
        let dir_light_lightness = Lcha::from(next_light.dir_color).lightness;
        let skybox_brightness =
            (next_light.dir_illuminance.sqrt() * 40.0 * dir_light_lightness).min(2000.0);
        if let Some(mut skybox) = maybe_skybox {
            skybox.brightness = skybox_brightness;
            atmosphere.rayleigh_coefficient =
                Vec3::new(5.5e-6, 13.0e-6, 22.4e-6) * next_light.dir_color.to_srgba().to_vec3();
        }

        if let Some(mut fog) = maybe_fog {
            let distance = (scene_distance.load + scene_distance.unload)
                .max(scene_distance.load_imposter * 0.333)
                + maybe_primary.map_or(0.0, |camera| camera.distance * 5.0);

            let base_color = next_light.ambient_color.to_srgba()
                * next_light.ambient_brightness
                * 0.5
                * skybox_brightness
                / 2000.0;
            let base_color = Color::from(base_color).with_alpha(1.0);

            fog.color = base_color;
            match setting.graphics.fog {
                FogSetting::Off => {
                    fog.falloff = FogFalloff::from_visibility_squared(distance * 200.0);
                    fog.directional_light_color = base_color;
                }
                FogSetting::Basic => {
                    fog.falloff = FogFalloff::from_visibility_squared(distance * 2.0);
                    fog.directional_light_color = base_color;
                }
                FogSetting::Atmospheric => {
                    fog.falloff = FogFalloff::from_visibility_squared(distance * 2.0);
                    fog.directional_light_color = next_light.dir_color;
                }
            }

            // let sun_up = atmosphere.sun_position.dot(Vec3::Y);
            // let rgb = Vec3::new(0.4, 0.4, 0.2) * sun_up.clamp(0.0, 1.0)
            //     + Vec3::new(0.0, 0.0, 0.0) * (8.0 * (0.125 - sun_up.clamp(0.0, 0.125)));
            // let rgb = rgb.powf(1.0 / 2.2);
            // fog.color = Color::srgb(rgb.x, rgb.y, rgb.z);
        }
    }

    ambient.brightness =
        next_light.ambient_brightness * config.graphics.ambient_brightness as f32 * 20.0;
    ambient.color = next_light.ambient_color;

    if prev.1.source == scene_global_light.source {
        prev.0 += time.delta_seconds()
    } else {
        prev.0 = time.delta_seconds()
    };
    prev.1 = next_light;
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

    transform.translation = target.translation() * Vec3::new(1.0, 0.0, 1.0) + Vec3::Y * -0.05;
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/shadows")]
struct ShadowConsoleCommand {
    on: Option<bool>,
}

fn shadow_console_command(
    mut input: ConsoleCommand<ShadowConsoleCommand>,
    mut lights: Query<&mut DirectionalLight>,
) {
    if let Some(Ok(command)) = input.take() {
        for mut light in lights.iter_mut() {
            light.shadows_enabled = command.on.unwrap_or(!light.shadows_enabled);
        }

        input.reply_ok(format!(
            "shadows {}",
            match command.on {
                None => "toggled",
                Some(true) => "enabled",
                Some(false) => "disabled",
            }
        ));
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/fog")]
struct FogConsoleCommand {
    on: Option<bool>,
}

fn fog_console_command(
    mut input: ConsoleCommand<FogConsoleCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        let activate = command.on.unwrap_or(true);

        config.graphics.fog = if activate {
            FogSetting::Atmospheric
        } else {
            FogSetting::Off
        };

        input.reply_ok(format!(
            "fog {}",
            match activate {
                true => "enabled",
                false => "disabled",
            }
        ));
    }
}
