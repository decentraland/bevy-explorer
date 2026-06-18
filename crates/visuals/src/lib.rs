mod day_night;
pub mod env_downsample;
mod nishita_cloud;
pub mod shell_texturing;

use bevy::{
    core_pipeline::dof::{DepthOfField, DepthOfFieldMode},
    pbr::{wireframe::WireframePlugin, CascadeShadowConfigBuilder, DirectionalLightShadowMap},
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
use nishita_cloud::{
    build_sky_lut, init_noise, load_clouds_strip, load_unity_clouds, load_unity_moon,
    load_unity_stars, load_unity_sun, NishitaCloud, SkyColorTuning, SkyLut,
};

use bevy_console::ConsoleCommand;
use common::{
    sets::SetupSets,
    structs::{
        AppConfig, DofConfig, FogSetting, PrimaryCamera, PrimaryCameraRes, PrimaryUser,
        SceneGlobalLight, SceneLoadDistance, ShadowSetting, PRIMARY_AVATAR_LIGHT_LAYER,
    },
};
use console::DoAddConsoleCommand;
// use env_downsample::{Envmap, EnvmapDownsamplePlugin};

use crate::{day_night::DayNightPlugin, shell_texturing::ShellTexturingPlugin};

pub struct VisualsPlugin {
    pub no_fog: bool,
}

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 4096 })
            .init_resource::<SceneGlobalLight>()
            .init_resource::<SkyColorTuning>()
            .insert_resource(CloudCover {
                // procedural clouds default off — the painted godot cloud
                // cubemap is the primary cloud layer now; /cloud re-enables
                cover: 0.0,
                speed: 10.0,
            })
            .insert_resource(CloudDrift { speed: 0.008 })
            .add_plugins(WireframePlugin::default())
            .add_plugins(DayNightPlugin)
            .add_plugins(ShellTexturingPlugin)
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
            app.insert_resource(RenderAssetBytesPerFrame::new_with_priorities(
                config.graphics.gpu_bytes_per_frame,
            ));
        }

        // app.add_plugins(EnvmapDownsamplePlugin);

        app.add_console_command::<ShadowConsoleCommand, _>(shadow_console_command);
        app.add_console_command::<FogConsoleCommand, _>(fog_console_command);
        app.add_console_command::<DofConsoleCommand, _>(dof_console_command);
        app.add_console_command::<CloudConsoleCommand, _>(cloud_console_command);
        app.add_console_command::<TonemapConsoleCommand, _>(tonemap_console_command);
        app.add_console_command::<ExposureConsoleCommand, _>(exposure_console_command);
        app.add_console_command::<GammaConsoleCommand, _>(gamma_console_command);
        app.add_console_command::<SaturationConsoleCommand, _>(saturation_console_command);
        app.add_console_command::<BloomConsoleCommand, _>(bloom_console_command);
        app.add_console_command::<AmbientConsoleCommand, _>(ambient_console_command);
        app.add_console_command::<SoftShadowConsoleCommand, _>(softshadow_console_command);
        app.add_console_command::<AmbientTintConsoleCommand, _>(ambienttint_console_command);
        app.add_console_command::<SunConsoleCommand, _>(sun_console_command);
        app.add_console_command::<SkyZenithConsoleCommand, _>(skyzenith_console_command);
        app.add_console_command::<SkyHorizonConsoleCommand, _>(skyhorizon_console_command);
        app.add_console_command::<SkyNadirConsoleCommand, _>(skynadir_console_command);
        app.add_console_command::<SkyGainConsoleCommand, _>(skygain_console_command);
        app.add_console_command::<SkySatConsoleCommand, _>(skysat_console_command);
        app.add_console_command::<SkyCloudsConsoleCommand, _>(skyclouds_console_command);
        app.add_console_command::<CloudSpinConsoleCommand, _>(cloudspin_console_command);
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
    camera: Res<PrimaryCameraRes>,
    mut atmosphere: AtmosphereMut<NishitaCloud>,
    mut images: ResMut<Assets<Image>>,
    mut sky_materials: ResMut<Assets<bevy_atmosphere::skybox::SkyBoxMaterial>>,
    sky_material: Res<bevy_atmosphere::skybox::AtmosphereSkyBoxMaterial>,
    sky_tuning: Res<SkyColorTuning>,
    // envmap: Res<Envmap>,
) {
    info!("visuals::setup");

    commands.entity(camera.0).try_insert(DistanceFog {
        color: Color::srgb(0.3, 0.2, 0.1),
        directional_light_color: Color::srgb(1.0, 1.0, 0.7),
        directional_light_exponent: 10.0,
        falloff: FogFalloff::ExponentialSquared { density: 0.01 },
    });

    {
        commands.entity(camera.0).try_insert(AtmosphereCamera {
            render_layers: Some(RenderLayers::default()),
        });

        let noise = init_noise(512);
        let h_noise = images.add(noise);

        atmosphere.noise_texture = h_noise;
        atmosphere.sky_lut = images.add(build_sky_lut(&sky_tuning));
        // keep the handle so /sky* commands can rebuild the LUT pixels live
        commands.insert_resource(SkyLut(atmosphere.sky_lut.clone()));
        atmosphere.clouds_strip = images.add(load_clouds_strip());

        // the visible sky is rendered per-pixel by the skybox material; give
        // it the same color-cycle lut and painted clouds
        if let Some(material) = sky_materials.get_mut(&sky_material.0) {
            material.sky_lut = atmosphere.sky_lut.clone();
            // clouds use the godot painted CUBEMAP (6x1 face strip) — it is
            // pole-free, so horizon clouds rise cleanly with no zenith stretch,
            // unlike the equirectangular unity panorama.
            material.clouds_strip = images.add(load_clouds_strip());
            material.sun_sprite = images.add(load_unity_sun());
            material.moon_sprite = images.add(load_unity_moon());
            material.stars_tex = images.add(load_unity_stars());
        }
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
    time_of_day: Res<common::structs::TimeOfDay>,
    mut sky_materials: ResMut<Assets<bevy_atmosphere::skybox::SkyBoxMaterial>>,
    sky_material: Res<bevy_atmosphere::skybox::AtmosphereSkyBoxMaterial>,
    mut prev: Local<(f32, SceneGlobalLight)>,
    config: Res<AppConfig>,
    // grouped into one tuple param to stay under Bevy's 16-param system limit
    (mut cloud_dt, cloud_drift, mut cloud_angle): (
        Local<f32>,
        Res<CloudDrift>,
        Local<f32>,
    ),
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
            fog_color: (scene_global_light.fog_color.to_srgba() * new_amount
                + prev.1.fog_color.to_srgba() * old_amount)
                .into(),
            layers: scene_global_light.layers.clone(),
        }
    };

    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, next_light.dir_direction);
    atmosphere.sun_position = -next_light.dir_direction;
    atmosphere.rayleigh_coefficient =
        Vec3::new(5.5e-6, 13.0e-6, 22.4e-6) * next_light.dir_color.to_srgba().to_vec3();
    atmosphere.dir_light_intensity = next_light.dir_illuminance;
    atmosphere.sun_color = next_light.dir_color.to_srgba().to_vec3();
    // drive the color-cycle lut with the time of day
    atmosphere.day = (time_of_day.elapsed_secs() / (60.0 * 60.0 * 24.0)).rem_euclid(1.0);
    atmosphere.tick += 1;

    // push sun direction + time into the per-pixel skybox material
    if let Some(material) = sky_materials.get_mut(&sky_material.0) {
        material.data = atmosphere.sun_position.extend(atmosphere.day);
        // slow Unity-style cloud drift: accumulate the rotation angle on
        // wall-clock time (keeps moving even while the day cycle is frozen for
        // tuning), at the live /cloudspin speed.
        *cloud_angle = (*cloud_angle + time.delta_secs() * cloud_drift.speed)
            .rem_euclid(std::f32::consts::TAU);
        material.extra.x = *cloud_angle;
    }

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

    // neutralize the sun/moon light COLOR toward white so environment props
    // keep their own albedo instead of being washed by the sky hue (unity
    // parity — the same idea the avatar toon shader uses by ignoring light
    // color). controlled by the same /ambienttint knob: 1.0 = full sky color,
    // 0.0 = pure white light.
    let tint = config.graphics.ambient_tint.clamp(0.0, 1.0);
    let ds = next_light.dir_color.to_linear();
    let tinted_dir_color = Color::from(LinearRgba::new(
        ds.red * tint + (1.0 - tint),
        ds.green * tint + (1.0 - tint),
        ds.blue * tint + (1.0 - tint),
        1.0,
    ));

    let mut directional_layers = RenderLayers::none();
    for (entity, layer, mut light_trans, mut directional) in sun.iter_mut() {
        if !next_light.layers.intersects(&RenderLayers::layer(layer.0)) {
            commands.entity(entity).despawn();
            continue;
        }

        directional_layers = directional_layers.with(layer.0);
        light_trans.rotation = rotation;
        directional.illuminance = next_light.dir_illuminance * config.graphics.sun_intensity;
        directional.color = tinted_dir_color;
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
            DirectionalLight {
                color: tinted_dir_color,
                illuminance: next_light.dir_illuminance * config.graphics.sun_intensity,
                shadows_enabled,
                // soft (PCSS) shadow penumbra for unity-like soft edges;
                // scrub live with /softshadow. larger = softer.
                soft_shadow_size: Some(config.graphics.soft_shadow_size),
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
        // floor keeps night fog violet instead of black
        let skybox_brightness =
            (next_light.dir_illuminance.sqrt() * 40.0 * dir_light_lightness).clamp(400.0, 2000.0);

        if let Some(mut fog) = maybe_fog {
            let distance = (scene_distance.load + scene_distance.unload)
                .max(scene_distance.load_imposter * 0.333)
                + maybe_primary.map_or(0.0, |camera| camera.distance * 5.0);

            // fog tint follows its own day-cycle gradient (godot parity),
            // scaled by overall sky brightness so night fog goes dark
            let base_color = next_light.fog_color.to_srgba()
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
    // blend the sky-tinted ambient toward neutral white so environment assets
    // keep their own colors (unity-like) instead of being washed with the sky
    // hue. tint=1 -> full sky color, tint=0 -> white. scrub with /ambienttint
    let tint = config.graphics.ambient_tint.clamp(0.0, 1.0);
    let sky = next_light.ambient_color.to_linear();
    ambient.color = Color::from(LinearRgba::new(
        sky.red * tint + (1.0 - tint),
        sky.green * tint + (1.0 - tint),
        sky.blue * tint + (1.0 - tint),
        1.0,
    ));

    if prev.1.source == scene_global_light.source {
        prev.0 += time.delta_secs()
    } else {
        prev.0 = time.delta_secs()
    };
    prev.1 = next_light;
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

/// drift speed (radians/sec) of the painted cloud layer rotating around the
/// vertical axis. live-tunable via /cloudspin.
#[derive(Resource)]
pub struct CloudDrift {
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

// --- environment grading commands ---
// the unity and godot clients both build their environment look on standard
// PBR + a tuned post stack (both use ACES tonemapping; godot adds glow and
// sky-driven ambient). these commands expose the bevy post stack for live
// tuning so the environment mood can be matched by eye.

/// set the tonemapping curve.
/// options: none, reinhard, reinhard_luma, aces, agx, sbdt, tmmf (default), blender
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/tonemap")]
struct TonemapConsoleCommand {
    mode: String,
}

fn tonemap_console_command(
    mut input: ConsoleCommand<TonemapConsoleCommand>,
    mut cam: Query<&mut bevy::core_pipeline::tonemapping::Tonemapping, With<PrimaryCamera>>,
) {
    use bevy::core_pipeline::tonemapping::Tonemapping;
    if let Some(Ok(command)) = input.take() {
        let Ok(mut tonemapping) = cam.single_mut() else {
            return;
        };
        let mode = match command.mode.as_str() {
            "none" => Tonemapping::None,
            "reinhard" => Tonemapping::Reinhard,
            "reinhard_luma" => Tonemapping::ReinhardLuminance,
            "aces" => Tonemapping::AcesFitted,
            "agx" => Tonemapping::AgX,
            "sbdt" => Tonemapping::SomewhatBoringDisplayTransform,
            "tmmf" => Tonemapping::TonyMcMapface,
            "blender" => Tonemapping::BlenderFilmic,
            other => {
                input.reply_failed(format!("unknown mode `{other}`; options: none, reinhard, reinhard_luma, aces, agx, sbdt, tmmf, blender"));
                return;
            }
        };
        *tonemapping = mode;
        input.reply_ok(format!("tonemapping: {}", command.mode));
    }
}

/// set global exposure (stops, default -0.5)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/exposure")]
struct ExposureConsoleCommand {
    exposure: f32,
}

fn exposure_console_command(
    mut input: ConsoleCommand<ExposureConsoleCommand>,
    mut cam: Query<&mut bevy::render::view::ColorGrading, With<PrimaryCamera>>,
) {
    if let Some(Ok(command)) = input.take() {
        let Ok(mut grading) = cam.single_mut() else {
            return;
        };
        grading.global.exposure = command.exposure;
        input.reply_ok(format!("exposure: {}", command.exposure));
    }
}

/// set gamma per tonal range (defaults 0.75 0.75 0.75); 1.0 = neutral
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/gamma")]
struct GammaConsoleCommand {
    shadows: f32,
    midtones: Option<f32>,
    highlights: Option<f32>,
}

fn gamma_console_command(
    mut input: ConsoleCommand<GammaConsoleCommand>,
    mut cam: Query<&mut bevy::render::view::ColorGrading, With<PrimaryCamera>>,
) {
    if let Some(Ok(command)) = input.take() {
        let Ok(mut grading) = cam.single_mut() else {
            return;
        };
        let midtones = command.midtones.unwrap_or(command.shadows);
        let highlights = command.highlights.unwrap_or(midtones);
        grading.shadows.gamma = command.shadows;
        grading.midtones.gamma = midtones;
        grading.highlights.gamma = highlights;
        input.reply_ok(format!(
            "gamma: shadows {} midtones {} highlights {}",
            command.shadows, midtones, highlights
        ));
    }
}

/// set saturation per tonal range (default 1.0); 0 = grayscale
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/saturation")]
struct SaturationConsoleCommand {
    shadows: f32,
    midtones: Option<f32>,
    highlights: Option<f32>,
}

fn saturation_console_command(
    mut input: ConsoleCommand<SaturationConsoleCommand>,
    mut cam: Query<&mut bevy::render::view::ColorGrading, With<PrimaryCamera>>,
) {
    if let Some(Ok(command)) = input.take() {
        let Ok(mut grading) = cam.single_mut() else {
            return;
        };
        let midtones = command.midtones.unwrap_or(command.shadows);
        let highlights = command.highlights.unwrap_or(midtones);
        grading.shadows.saturation = command.shadows;
        grading.midtones.saturation = midtones;
        grading.highlights.saturation = highlights;
        input.reply_ok(format!(
            "saturation: shadows {} midtones {} highlights {}",
            command.shadows, midtones, highlights
        ));
    }
}

/// set bloom intensity (default 0.15, godot reference ~0.25-0.3; 0 = off)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/bloom")]
struct BloomConsoleCommand {
    intensity: f32,
}

fn bloom_console_command(
    mut input: ConsoleCommand<BloomConsoleCommand>,
    mut cam: Query<&mut bevy::core_pipeline::bloom::Bloom, With<PrimaryCamera>>,
) {
    if let Some(Ok(command)) = input.take() {
        let Ok(mut bloom) = cam.single_mut() else {
            return;
        };
        bloom.intensity = command.intensity;
        input.reply_ok(format!("bloom intensity: {}", command.intensity));
    }
}

/// multiplier on the sun's brightness (default 1.0). raise to light
/// environment props up more (unity-like); e.g. 1.5–3.0.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/sun")]
struct SunConsoleCommand {
    intensity: f32,
}

fn sun_console_command(
    mut input: ConsoleCommand<SunConsoleCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        config.graphics.sun_intensity = command.intensity;
        input.reply_ok(format!("sun intensity: {}", command.intensity));
    }
}

// --- live sky color tuning -----------------------------------------------
// the sky dome is a tri-zone gradient (zenith -> horizon -> nadir) built from
// the measured Unity colors in common::godot_sky. these commands apply live
// RGB gains per zone (default 1 1 1 = measured colors verbatim) and rebuild
// the sky LUT in place so the change shows immediately. tune by eye against a
// Unity reference, then bake the values you settle on into SkyColorTuning's
// Default impl in nishita_cloud.rs.

/// rebuild the sky color LUT pixels from the current gradients + tuning.
fn rebuild_sky_lut(
    tuning: &SkyColorTuning,
    lut: &SkyLut,
    images: &mut Assets<Image>,
) {
    if let Some(image) = images.get_mut(&lut.0) {
        *image = build_sky_lut(tuning);
    }
}

/// RGB gain on the zenith (straight-up) sky color. default 1 1 1.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skyzenith")]
struct SkyZenithConsoleCommand {
    r: f32,
    g: f32,
    b: f32,
}

fn skyzenith_console_command(
    mut input: ConsoleCommand<SkyZenithConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.zenith = Vec3::new(command.r, command.g, command.b);
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!(
            "sky zenith gain: {} {} {}",
            command.r, command.g, command.b
        ));
    }
}

/// RGB gain on the horizon sky color. default 1 1 1.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skyhorizon")]
struct SkyHorizonConsoleCommand {
    r: f32,
    g: f32,
    b: f32,
}

fn skyhorizon_console_command(
    mut input: ConsoleCommand<SkyHorizonConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.horizon = Vec3::new(command.r, command.g, command.b);
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!(
            "sky horizon gain: {} {} {}",
            command.r, command.g, command.b
        ));
    }
}

/// RGB gain on the nadir (below-horizon) sky color. default 1 1 1.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skynadir")]
struct SkyNadirConsoleCommand {
    r: f32,
    g: f32,
    b: f32,
}

fn skynadir_console_command(
    mut input: ConsoleCommand<SkyNadirConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.nadir = Vec3::new(command.r, command.g, command.b);
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!(
            "sky nadir gain: {} {} {}",
            command.r, command.g, command.b
        ));
    }
}

/// master brightness multiplier on all three sky zones at once. default 1.0.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skygain")]
struct SkyGainConsoleCommand {
    master: f32,
}

fn skygain_console_command(
    mut input: ConsoleCommand<SkyGainConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.master = command.master;
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!("sky master gain: {}", command.master));
    }
}

/// sky-only color vibrancy. default 1.4. 1.0 = measured colors (washed out),
/// higher = more saturated/vivid blue, lower = greyer. affects ONLY the sky,
/// not the rest of the scene (unlike /saturation).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skysat")]
struct SkySatConsoleCommand {
    amount: f32,
}

fn skysat_console_command(
    mut input: ConsoleCommand<SkySatConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.sat = command.amount;
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!("sky saturation: {}", command.amount));
    }
}

/// cloud drift speed in radians/sec (default 0.008 ≈ 13 min per full turn).
/// 0 = stop. higher = faster. e.g. /cloudspin 0.02 for a quicker drift.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloudspin")]
struct CloudSpinConsoleCommand {
    speed: f32,
}

fn cloudspin_console_command(
    mut input: ConsoleCommand<CloudSpinConsoleCommand>,
    mut drift: ResMut<CloudDrift>,
) {
    if let Some(Ok(command)) = input.take() {
        drift.speed = command.speed;
        input.reply_ok(format!("cloud drift speed: {} rad/s", command.speed));
    }
}

/// cloud horizon band. `/skyclouds <horizon> [top]`.
/// horizon (default 0.47) = texture row on the horizon (shifts the band's
/// content up/down). top (default 0.5) = how high the cloud band reaches
/// (ray.y, 0 = horizon .. 1 = straight up); clear sky above it. keep top low
/// (≈0.4–0.6) so clouds stay near the horizon and never stretch at the zenith.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/skyclouds")]
struct SkyCloudsConsoleCommand {
    horizon: f32,
    vscale: Option<f32>,
}

fn skyclouds_console_command(
    mut input: ConsoleCommand<SkyCloudsConsoleCommand>,
    mut tuning: ResMut<SkyColorTuning>,
    lut: Option<Res<SkyLut>>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(Ok(command)) = input.take() {
        tuning.cloud_horizon = command.horizon;
        if let Some(vscale) = command.vscale {
            tuning.cloud_vscale = vscale;
        }
        if let Some(lut) = lut.as_deref() {
            rebuild_sky_lut(&tuning, lut, &mut images);
        }
        input.reply_ok(format!(
            "sky clouds: horizon {}, vscale {}",
            command.horizon, tuning.cloud_vscale
        ));
    }
}

/// how much the sky color tints the world's lighting — BOTH the ambient fill
/// and the sun/moon directional color (default 1.0).
/// 0 = neutral white light (assets keep their own colors, unity-like);
/// 1 = full sky color (everything washed with the skybox hue).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/ambienttint")]
struct AmbientTintConsoleCommand {
    amount: f32,
}

fn ambienttint_console_command(
    mut input: ConsoleCommand<AmbientTintConsoleCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        config.graphics.ambient_tint = command.amount;
        input.reply_ok(format!("ambient tint: {}", command.amount));
    }
}

/// set the soft-shadow penumbra size in world units (default 4.0).
/// 0 = crisp/hard edges; larger = softer, unity-like shadow edges.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/softshadow")]
struct SoftShadowConsoleCommand {
    size: f32,
}

fn softshadow_console_command(
    mut input: ConsoleCommand<SoftShadowConsoleCommand>,
    mut config: ResMut<AppConfig>,
    mut lights: Query<&mut DirectionalLight>,
) {
    if let Some(Ok(command)) = input.take() {
        config.graphics.soft_shadow_size = command.size;
        for mut light in lights.iter_mut() {
            light.soft_shadow_size = (command.size > 0.0).then_some(command.size);
        }
        input.reply_ok(format!("soft shadow size: {}", command.size));
    }
}

/// set the flat ambient-fill level (default 50). lower = more contrast/shape
/// on environment models (the sky/directional light does more of the work);
/// higher = flatter, more evenly lit. unity-parity target is much lower.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/ambient")]
struct AmbientConsoleCommand {
    level: i32,
}

fn ambient_console_command(
    mut input: ConsoleCommand<AmbientConsoleCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        config.graphics.ambient_brightness = command.level;
        input.reply_ok(format!("ambient fill: {}", command.level));
    }
}
