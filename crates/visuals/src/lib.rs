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
use nishita_cloud::{init_noise, NishitaCloud};

use bevy_console::ConsoleCommand;
use common::{
    sets::SetupSets,
    sky_params,
    structs::{
        AppConfig, DofConfig, FogSetting, PrimaryCamera, PrimaryCameraRes, PrimaryUser,
        SceneGlobalLight, SceneLoadDistance, ShadowSetting, TimeOfDay, PRIMARY_AVATAR_LIGHT_LAYER,
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
            .insert_resource(CloudCover {
                cover: 0.35,
                speed: 30.0,
                density_cap: 0.8,
                shadow: 0.05,
                scale: 1.5,
                steps: 44,
                lacunarity: 2.0,
            })
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
        app.add_console_command::<CloudDensityConsoleCommand, _>(cloud_density_console_command);
        app.add_console_command::<CloudShadowConsoleCommand, _>(cloud_shadow_console_command);
        app.add_console_command::<CloudScaleConsoleCommand, _>(cloud_scale_console_command);
        app.add_console_command::<CloudStepsConsoleCommand, _>(cloud_steps_console_command);
        app.add_console_command::<CloudLacunarityConsoleCommand, _>(
            cloud_lacunarity_console_command,
        );
        app.add_console_command::<TonemapConsoleCommand, _>(tonemap_console_command);
        app.add_console_command::<ExposureConsoleCommand, _>(exposure_console_command);
        app.add_console_command::<GammaConsoleCommand, _>(gamma_console_command);
        app.add_console_command::<SaturationConsoleCommand, _>(saturation_console_command);
        app.add_console_command::<BloomConsoleCommand, _>(bloom_console_command);
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
    time_of_day: Res<TimeOfDay>,
    mut prev: Local<(f32, SceneGlobalLight)>,
    mut cloud_dt: Local<f32>,
    mut last_primary_distance: Local<f32>,
) {
    // the transition has settled once the previous output exactly matches the target
    let settled = prev.0 >= TRANSITION_TIME && prev.1 == *scene_global_light;

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

    // physically-simulated sky: rayleigh (hue) and mie (haze) are baked day-cycle
    // curves keyed by time of day; the sun sets naturally (no floor), and a flat
    // night colour (added in-shader) provides the night sky.
    let day = (time_of_day.elapsed_secs() / (60.0 * 60.0 * 24.0)).rem_euclid(1.0);
    atmosphere.sun_position = -next_light.dir_direction;
    atmosphere.rayleigh_coefficient = sky_params::RAYLEIGH.sample(day);
    atmosphere.mie_coefficient = sky_params::MIE.sample(day);
    atmosphere.night_color = sky_params::NIGHT_SKY;
    // moon on its own low orbit: rises at dusk, peaks at MOON_PEAK_ELEV around
    // midnight (well below the zenith, so it never sits overhead like the sun),
    // sets at dawn. Anti-phase to the sun but on an independent arc, so it has
    // no singularity at midnight (where the antisolar direction is undefined).
    const MOON_PEAK_ELEV: f32 = 0.45; // radians (~26°)
    let a = day * std::f32::consts::TAU + std::f32::consts::FRAC_PI_2;
    let (sin_a, cos_a) = a.sin_cos();
    let (sin_b, cos_b) = MOON_PEAK_ELEV.sin_cos();
    atmosphere.moon_position = Vec3::new(cos_a, sin_a * sin_b, -sin_a * cos_b);
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

    // cloud look (live-tunable, baked later)
    atmosphere.cloud_density_cap = cloud.density_cap;
    atmosphere.cloud_shadow = cloud.shadow;
    atmosphere.cloud_scale = cloud.scale;
    atmosphere.cloud_steps = cloud.steps;
    atmosphere.cloud_lacunarity = cloud.lacunarity;

    // skip the light/fog/ambient writes (which trigger change detection and re-extraction)
    // when the light has settled and nothing else affecting them has changed
    let primary_distance = cameras
        .iter()
        .find_map(|(maybe_primary, _)| maybe_primary.map(|camera| camera.distance))
        .unwrap_or(0.0);
    // a (re)inserted DistanceFog fires `is_added` and must be written even when the light has
    // settled. reading the tick through the existing `&mut` access avoids the query conflict an
    // `Added<DistanceFog>` filter would have with `cameras`; `is_added` (not `is_changed`) is used
    // because our own per-frame fog writes set `changed`, which would otherwise never let the gate
    // re-engage.
    let fog_added = cameras
        .iter_mut()
        .any(|(_, fog)| fog.is_some_and(|fog| fog.is_added()));
    let skip_writes = settled
        && !setting.is_changed()
        && !scene_distance.is_changed()
        && !fog_added
        && *last_primary_distance == primary_distance;
    *last_primary_distance = primary_distance;

    if skip_writes {
        prev.0 += time.delta_secs();
        prev.1 = next_light;
        return;
    }

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

        let (shadows_enabled, cascade_shadow_config) = match setting.graphics.shadow_settings {
            ShadowSetting::Off => (false, Default::default()),
            ShadowSetting::Low => (
                true,
                CascadeShadowConfigBuilder {
                    num_cascades: 1,
                    minimum_distance: 0.1,
                    maximum_distance: setting.graphics.shadow_distance,
                    first_cascade_far_bound: setting.graphics.shadow_distance,
                    overlap_proportion: 0.2,
                }
                .build(),
            ),
            ShadowSetting::High => (
                true,
                CascadeShadowConfigBuilder {
                    num_cascades: 4,
                    minimum_distance: 0.1,
                    maximum_distance: setting.graphics.shadow_distance,
                    first_cascade_far_bound: setting.graphics.shadow_distance / 15.0,
                    overlap_proportion: 0.2,
                }
                .build(),
            ),
        };

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
        // floor keeps night fog tinted instead of going black
        let skybox_brightness =
            (next_light.dir_illuminance.sqrt() * 40.0 * dir_light_lightness).clamp(400.0, 2000.0);

        if let Some(mut fog) = maybe_fog {
            let distance = (scene_distance.load + scene_distance.unload)
                .max(scene_distance.load_imposter * 0.333)
                + maybe_primary.map_or(0.0, |camera| camera.distance * 5.0);

            // fog tint follows its own day-cycle gradient, scaled by overall
            // sky brightness so night fog goes dark
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
        next_light.ambient_brightness * setting.graphics.ambient_brightness as f32 * 20.0;
    ambient.color = next_light.ambient_color;

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
    /// accumulated density that reads as fully opaque (lower = thicker clouds).
    pub density_cap: f32,
    /// shadow / minimum cloud brightness (the dark side of clouds).
    pub shadow: f32,
    /// cloud noise sample scale (higher = finer/smaller features).
    pub scale: f32,
    /// cloud ray-march step count (higher = smoother/more detail, more cost).
    pub steps: u32,
    /// per-octave frequency step of the cloud noise (default 2.345).
    pub lacunarity: f32,
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

/// max accumulated density that reads as fully opaque; lower = thicker clouds.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/clouddensity")]
struct CloudDensityConsoleCommand {
    cap: f32,
}

fn cloud_density_console_command(
    mut input: ConsoleCommand<CloudDensityConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.density_cap = command.cap;
        input.reply_ok(format!("cloud density cap {}", command.cap));
    }
}

/// shadow / minimum cloud brightness (the dark side of clouds).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloudshadow")]
struct CloudShadowConsoleCommand {
    value: f32,
}

fn cloud_shadow_console_command(
    mut input: ConsoleCommand<CloudShadowConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.shadow = command.value;
        input.reply_ok(format!("cloud shadow {}", command.value));
    }
}

/// cloud noise sample scale; higher = finer/smaller features, lower = larger.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloudscale")]
struct CloudScaleConsoleCommand {
    scale: f32,
}

fn cloud_scale_console_command(
    mut input: ConsoleCommand<CloudScaleConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.scale = command.scale;
        input.reply_ok(format!("cloud scale {}", command.scale));
    }
}

/// cloud ray-march step count; higher = smoother/more detail, more cost.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloudsteps")]
struct CloudStepsConsoleCommand {
    steps: u32,
}

fn cloud_steps_console_command(
    mut input: ConsoleCommand<CloudStepsConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.steps = command.steps.max(1);
        input.reply_ok(format!("cloud steps {}", cloud.steps));
    }
}

/// per-octave frequency step of the cloud noise (how much finer each successive
/// wave is); default 2.345.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cloudlacunarity")]
struct CloudLacunarityConsoleCommand {
    lacunarity: f32,
}

fn cloud_lacunarity_console_command(
    mut input: ConsoleCommand<CloudLacunarityConsoleCommand>,
    mut cloud: ResMut<CloudCover>,
) {
    if let Some(Ok(command)) = input.take() {
        cloud.lacunarity = command.lacunarity;
        input.reply_ok(format!("cloud lacunarity {}", command.lacunarity));
    }
}

// --- environment grading commands ---
// expose the bevy post stack (tonemap + color grading + bloom) for live
// tuning so the environment mood can be matched by eye against a reference.

/// set the tonemapping curve.
/// options: none, reinhard, reinhard_luma, aces (default), agx, sbdt, tmmf, blender
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

/// set global exposure (stops, default 0.0)
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

/// set gamma per tonal range (default 1.0 = neutral). `/gamma <shadows> [midtones] [highlights]`
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

/// set saturation per tonal range (scene default 1.3); 0 = grayscale.
/// `/saturation <shadows> [midtones] [highlights]`
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

/// set bloom intensity (default 0.15; 0 = off)
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
