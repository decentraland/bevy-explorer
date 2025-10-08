pub mod env_downsample;
mod nishita_cloud;

use bevy::{
    core_pipeline::dof::{DepthOfField, DepthOfFieldMode},
    pbr::{wireframe::WireframePlugin, DirectionalLightShadowMap},
    prelude::*,
    render::{
        render_asset::RenderAssetBytesPerFrame,
        view::{Layer, RenderLayers},
    },
};

use bevy::render::RenderApp;
use bevy_atmosphere::{
    model::AddAtmosphereModel,
    pipeline::AtmosphereImageBindGroupLayout,
    prelude::{AtmosphereCamera, AtmosphereModel, AtmospherePlugin, AtmosphereSettings},
    system_param::AtmosphereMut,
};
use nishita_cloud::{init_noise, NishitaCloud};

use bevy_console::ConsoleCommand;
use common::{
    sets::SetupSets,
    structs::{
        AppConfig, DofConfig, FogSetting, PrimaryCamera, PrimaryCameraRes, PrimaryUser,
        SceneGlobalLight, SceneLoadDistance, TimeOfDay, GROUND_RENDERLAYER,
        PRIMARY_AVATAR_LIGHT_LAYER,
    },
};
use console::DoAddConsoleCommand;
// use env_downsample::{Envmap, EnvmapDownsamplePlugin};

pub struct VisualsPlugin {
    pub no_fog: bool,
}

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        // Initialize with default shadow map size - will be updated by shadow settings
        app.insert_resource(DirectionalLightShadowMap { size: 1024 })
            .init_resource::<SceneGlobalLight>()
            .insert_resource(TimeOfDay {
                time: 10.0 * 3600.0,
                target_time: None,
                speed: 12.0,
            })
            .insert_resource(CloudCover {
                cover: 0.45,
                speed: 10.0,
            })
            .add_plugins(WireframePlugin::default())
            .add_systems(First, update_time_of_day.after(bevy::time::TimeSystem))
            .add_systems(Update, apply_global_light)
            .add_systems(Update, update_dof)
            .add_systems(Startup, setup.in_set(SetupSets::Main));

        app.insert_resource(AtmosphereSettings {
            resolution: 1024,
            dithering: true,
            skybox_creation_mode:
                bevy_atmosphere::settings::SkyboxCreationMode::FromProjectionFarWithFallback(
                    99999.0,
                ),
        })
        .insert_resource(AtmosphereModel::new(NishitaCloud::default()))
        .add_plugins(AtmospherePlugin);

        let config = app.world().resource::<AppConfig>();

        if config.graphics.gpu_bytes_per_frame > 0 {
            app.insert_resource(RenderAssetBytesPerFrame::new(
                config.graphics.gpu_bytes_per_frame,
            ));
        }

        // app.add_plugins(EnvmapDownsamplePlugin);

        app.add_console_command::<ShadowConsoleCommand, _>(shadow_console_command);
        app.add_console_command::<FogConsoleCommand, _>(fog_console_command);
        app.add_console_command::<DofConsoleCommand, _>(dof_console_command);
        app.add_console_command::<CloudConsoleCommand, _>(cloud_console_command);
        app.add_console_command::<TimeOfDayConsoleCommand, _>(timeofday_console_command);
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<AtmosphereImageBindGroupLayout>();

        app.add_atmosphere_model::<NishitaCloud>();
    }
}

#[derive(Component)]
struct DirectionalLightLayer(Layer);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Res<PrimaryCameraRes>,
    mut atmosphere: AtmosphereMut<NishitaCloud>,
    mut images: ResMut<Assets<Image>>,
    // envmap: Res<Envmap>,
) {
    info!("visuals::setup");

    commands.entity(camera.0).try_insert(DistanceFog {
        color: Color::srgb(0.3, 0.2, 0.1),
        directional_light_color: Color::srgb(1.0, 1.0, 0.7),
        directional_light_exponent: 10.0,
        falloff: FogFalloff::ExponentialSquared { density: 0.01 },
    });

    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(6500.0, 6500.0)
                    .subdivisions(10),
            ),
        ),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.45, 0.2),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            depth_bias: -100.0,
            fog_enabled: false,
            ..Default::default()
        })),
        Transform::from_translation(Vec3::Y * -0.05),
        Ground,
        GROUND_RENDERLAYER.clone(),
    ));

    {
        commands.entity(camera.0).try_insert(AtmosphereCamera {
            render_layers: Some(RenderLayers::default()),
        });

        let noise = init_noise(512);
        let h_noise = images.add(noise);

        atmosphere.noise_texture = h_noise;
    }

    // commands.entity(camera.0).try_insert(
    //     EnvironmentMapLight {
    //         diffuse_map: envmap.0.clone(),
    //         specular_map: envmap.0.clone(),
    //         intensity: 3000.0,
    //     }
    // );
}

static TRANSITION_TIME: f32 = 1.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_global_light(
    mut commands: Commands,
    setting: Res<AppConfig>,
    mut atmosphere: AtmosphereMut<NishitaCloud>,
    cloud: Res<CloudCover>,
    mut sun: Query<(
        Entity,
        &DirectionalLightLayer,
        &mut Transform,
        &mut DirectionalLight,
    )>,
    mut ambient: ResMut<AmbientLight>,
    time: Res<Time>,
    mut cameras: Query<(Option<&PrimaryCamera>, Option<&mut DistanceFog>), With<Camera3d>>,
    scene_distance: Res<SceneLoadDistance>,
    scene_global_light: Res<SceneGlobalLight>,
    mut prev: Local<(f32, SceneGlobalLight)>,
    config: Res<AppConfig>,
    mut cloud_dt: Local<f32>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
) {
    let next_light = if prev.0 >= TRANSITION_TIME && prev.1.source == scene_global_light.source {
        scene_global_light.clone()
    } else {
        // transition part way
        let new_amount = if prev.1.source == scene_global_light.source {
            (time.delta_secs() / (TRANSITION_TIME - prev.0)).clamp(0.0, 1.0)
        } else {
            time.delta_secs() / TRANSITION_TIME
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
    atmosphere.rayleigh_coefficient =
        Vec3::new(5.5e-6, 13.0e-6, 22.4e-6) * next_light.dir_color.to_srgba().to_vec3();
    atmosphere.dir_light_intensity = next_light.dir_illuminance;
    atmosphere.sun_color = next_light.dir_color.to_srgba().to_vec3();
    atmosphere.tick += 1;

    if atmosphere.cloudy != cloud.cover {
        *cloud_dt = (*cloud_dt + time.delta_secs() * 20.0)
            .min(80.0 * (atmosphere.cloudy - cloud.cover).abs())
            .max(1.0);
        atmosphere.cloudy += (cloud.cover - atmosphere.cloudy).clamp(
            -time.delta_secs() * 0.005 * *cloud_dt,
            time.delta_secs() * 0.005 * *cloud_dt,
        );
        // atmosphere.time += time.delta_secs() * 10.0;
    } else {
        *cloud_dt = f32::max(*cloud_dt - time.delta_secs(), cloud.speed);
    }

    atmosphere.time += time.delta_secs() * *cloud_dt;

    let mut directional_layers = RenderLayers::none();
    for (entity, layer, mut light_trans, mut directional) in sun.iter_mut() {
        if !next_light.layers.intersects(&RenderLayers::layer(layer.0)) {
            commands.entity(entity).despawn();
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

        let (shadows_enabled, cascade_shadow_config, shadow_map_size) = config
            .graphics
            .shadow_settings
            .to_shadow_config(config.graphics.shadow_distance);

        // Update shadow map resolution based on current shadow settings
        shadow_map.size = shadow_map_size;

        commands.spawn((
            DirectionalLight {
                color: next_light.dir_color,
                illuminance: next_light.dir_illuminance,
                shadows_enabled,
                ..Default::default()
            },
            Transform::default().with_rotation(rotation),
            cascade_shadow_config,
            layer,
            DirectionalLightLayer(new_layer),
        ));
    }

    for (maybe_primary, maybe_fog) in cameras.iter_mut() {
        let dir_light_lightness = Lcha::from(next_light.dir_color).lightness;
        let skybox_brightness =
            (next_light.dir_illuminance.sqrt() * 40.0 * dir_light_lightness).min(2000.0);

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
        }
    }

    ambient.brightness =
        next_light.ambient_brightness * config.graphics.ambient_brightness as f32 * 20.0;
    ambient.color = next_light.ambient_color;

    if prev.1.source == scene_global_light.source {
        prev.0 += time.delta_secs()
    } else {
        prev.0 = time.delta_secs()
    };
    prev.1 = next_light;
}

#[derive(Component)]
struct Ground;

fn update_time_of_day(time: Res<Time>, mut tod: ResMut<TimeOfDay>, mut t_delta: Local<f32>) {
    if let Some(target) = tod.target_time {
        let initial_time = tod.time;
        let seconds_diff = (target - tod.time) % (24.0 * 3600.0);
        let seconds_to_travel = (seconds_diff + 12.0 * 3600.0) % (24.0 * 3600.0) - (12.0 * 3600.0);
        let unwrapped_target = initial_time + seconds_to_travel;

        tod.time += *t_delta * time.delta_secs();

        const ACCEL: f32 = 4.0 * 3600.0;

        let total_change_min = *t_delta * 0.5 * (*t_delta / ACCEL);
        if (tod.time + total_change_min - unwrapped_target).signum()
            == (tod.time - unwrapped_target).signum()
        {
            *t_delta += time.delta_secs() * ACCEL * seconds_to_travel.signum();
        } else {
            // we overshoot at this speed, start slowing down
            *t_delta -= time.delta_secs() * ACCEL * seconds_to_travel.signum();
        }

        if (initial_time - target).signum() != (tod.time - target).signum() {
            tod.time = target;
            tod.target_time = None;
            *t_delta = 0.0;
        }

        debug!("time: {initial_time}, target: {:?}, secs_to_travel: {seconds_to_travel}, t_delta: {}, final: {}", target, *t_delta, tod.time);
    } else {
        let speed = tod.speed;
        tod.time += time.delta_secs() * speed;
        tod.time %= 3600.0 * 24.0;
        if tod.time < 0.0 {
            tod.time += 3600.0 * 24.0;
        }
    }
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

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dof")]
struct DofConsoleCommand {
    focal_distance_extra: f32,
    sensor_height: f32,
    fstops: f32,
    max_circle: f32,
    max_depth: f32,
    mode: usize,
}

fn dof_console_command(
    mut input: ConsoleCommand<DofConsoleCommand>,
    mut cam: Query<(&mut DepthOfField, &mut DofConfig)>,
) {
    if let Some(Ok(command)) = input.take() {
        let Ok((mut dof, mut mydof)) = cam.single_mut() else {
            return;
        };

        *dof = DepthOfField {
            mode: if command.mode == 0 {
                DepthOfFieldMode::Gaussian
            } else {
                DepthOfFieldMode::Bokeh
            },
            aperture_f_stops: command.fstops,
            max_circle_of_confusion_diameter: command.max_circle,
            max_depth: command.max_depth,
            ..Default::default()
        };
        mydof.default_sensor_height = command.sensor_height;
        mydof.extra_focal_distance = command.focal_distance_extra;
        input.reply_ok("");
    }
}

fn update_dof(
    mut cam: Query<(&Transform, &DofConfig, &mut DepthOfField), With<PrimaryCamera>>,
    player: Query<&Transform, With<PrimaryUser>>,
) {
    let (Ok((cam, mydof, mut dof)), Ok(player)) = (cam.single_mut(), player.single()) else {
        return;
    };

    // let base_distance = 10.0;
    let constant_distance = mydof.extra_focal_distance;
    let current_distance = ((cam.translation - (player.translation + Vec3::Y * 1.81)).length()
        + constant_distance)
        .min(100.0);

    dof.sensor_height = mydof.default_sensor_height;
    // * ((current_distance * (current_distance + constant_distance))
    //     / (base_distance * (base_distance + constant_distance)))
    //     .sqrt();
    dof.focal_distance = current_distance;
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloud")]
struct CloudConsoleCommand {
    cover: f32,
    speed: Option<f32>,
}

#[derive(Resource)]
pub struct CloudCover {
    pub cover: f32,
    pub speed: f32,
}

fn cloud_console_command(
    mut input: ConsoleCommand<CloudConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.cover = command.cover;

        if let Some(speed) = command.speed {
            cloud.speed = speed;
        }

        input.reply_ok(format!(
            "cloud cover {}, speed {}",
            command.cover, cloud.speed
        ));
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/time")]
pub struct TimeOfDayConsoleCommand {
    pub time: Option<f32>,
    pub speed: Option<f32>,
}

fn timeofday_console_command(
    mut input: ConsoleCommand<TimeOfDayConsoleCommand>,
    mut time: ResMut<TimeOfDay>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Some(hours) = command.time {
            time.target_time = Some(hours * 3600.0);
        }
        if let Some(speed) = command.speed {
            time.speed = speed;
        }

        let target = time.target_time.unwrap_or(time.time);
        input.reply_ok(format!(
            "time {}:{} -> {}:{}, speed {} (elapsed: {})",
            (time.time as u32 / 3600),
            time.time as u32 % 3600 / 60,
            (target as u32 / 3600),
            target as u32 % 3600 / 60,
            time.speed,
            time.elapsed_secs()
        ));
    }
}
