use std::f32::consts::PI;

use bevy::{
    math::FloatOrd,
    pbr::decal::clustered::{CubemapLayout, PointLightTexture, SpotLightTexture},
    prelude::*,
    render::view::RenderLayers,
};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser, SceneGlobalLight, TimeOfDay, PRIMARY_AVATAR_LIGHT_LAYER},
    util::TryPushChildrenEx,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::{TextureUnion, Vector3},
        sdk::components::{pb_light_source, PbGlobalLight, PbLightSource},
        Color3DclToBevy,
    },
    SceneComponentId,
};

use crate::{
    renderer_context::RendererSceneContext,
    update_world::material::{TextureResolveError, TextureResolver},
    ContainerEntity, ContainingScene,
};

use super::AddCrdtInterfaceExt;

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbLightSource, LightSource>(
            SceneComponentId::LIGHT_SOURCE,
            ComponentPosition::Any,
        );
        app.add_crdt_lww_component::<PbGlobalLight, GlobalLight>(
            SceneComponentId::GLOBAL_LIGHT,
            ComponentPosition::RootOnly,
        );
        app.add_systems(
            Update,
            (
                update_directional_light,
                update_point_lights,
                manage_shadow_casters,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component, Debug, Default)]
pub struct LightSource {
    pub enabled: bool,
    pub intensity: Option<f32>,
    pub shadow: Option<bool>,
    pub color: Option<Color>,
    pub range: Option<f32>,
    pub spotlight_angles: Option<(f32, f32)>,
    pub light_texture: Option<TextureUnion>,
}

impl From<PbLightSource> for LightSource {
    fn from(value: PbLightSource) -> Self {
        // rescale colors > 1 to 1, and multiply intensity by pow(max^2.2)
        // to match unity
        let color_scale = value
            .color
            .map(|c| c.r.max(c.g).max(c.b).max(1.0))
            .unwrap_or(1.0);

        Self {
            enabled: value.active.unwrap_or(true),
            intensity: Some(value.intensity.unwrap_or(16000.0) * color_scale.powf(2.2)),
            shadow: value.shadow,
            color: value.color.map(|c| {
                Color::linear_rgb(c.r / color_scale, c.g / color_scale, c.b / color_scale)
            }),
            range: value.range,
            spotlight_angles: if let Some(pb_light_source::Type::Spot(spot)) = value.r#type {
                Some((
                    spot.inner_angle.unwrap_or(21.8),
                    spot.outer_angle.unwrap_or(30.0),
                ))
            } else {
                None
            },
            light_texture: value.shadow_mask_texture,
        }
    }
}

#[derive(Component)]
pub struct GlobalLight {
    pub direction: Option<Vec3>,
    pub ambient_color: Option<Color>,
    pub ambient_brightness: Option<f32>,
}

impl From<PbGlobalLight> for GlobalLight {
    fn from(value: PbGlobalLight) -> Self {
        Self {
            direction: value.direction.as_ref().map(Vector3::world_vec_to_vec3),
            ambient_color: value.ambient_color.map(Color3DclToBevy::convert_linear_rgb),
            ambient_brightness: value.ambient_brightness,
        }
    }
}

/// Unity's sunrise and sunset (hours): the sun is overhead halfway between, at 12:37.
const SUNRISE: f32 = 6.422;
const SUNSET: f32 = 18.828;

/// Unity's moonrise and moonset (hours): the moon is up for six and a half hours around 00:45.
const MOONRISE: f32 = 21.504;
const MOONSET: f32 = 4.008;

/// Unity's moon arc leans this far from the zenith, so the moon stays low for long shadows.
const MOON_TILT_DEGREES: f32 = 40.0;

/// Direction a body's light travels at `hours` past midnight. Like Unity, the body rises where
/// the sun rises, crosses the sky at a steady rate to the opposite point on an arc leaning
/// `tilt_degrees` from the zenith, and carries on below the horizon to its next rise.
fn celestial_direction(hours: f32, rise: f32, set: f32, tilt_degrees: f32) -> Vec3 {
    let since_rise = (hours - rise).rem_euclid(24.0);
    let up = (set - rise).rem_euclid(24.0);
    // from the rise horizon: 0 to PI while up, PI to 2 PI while down
    let angle = if since_rise < up {
        since_rise / up * PI
    } else {
        PI + (since_rise - up) / (24.0 - up) * PI
    };
    // away from the body as it rises, and away from it at its peak
    let rising = Vec3::new(1.0, 0.0, -1.0).normalize();
    let (tilt_sin, tilt_cos) = tilt_degrees.to_radians().sin_cos();
    let peak = -(Vec3::Y * tilt_cos + Vec3::new(-1.0, 0.0, -1.0).normalize() * tilt_sin);
    rising * angle.cos() + peak * angle.sin()
}

/// Direction the sunlight travels at `hours` past midnight: straight over the zenith in a
/// vertical plane diagonal to the parcel grid.
fn sun_direction(hours: f32) -> Vec3 {
    celestial_direction(hours, SUNRISE, SUNSET, 0.0)
}

/// Direction the moonlight travels at `hours` past midnight: the sun's arc leaned to one side,
/// crossed overnight.
pub fn moon_direction(hours: f32) -> Vec3 {
    celestial_direction(hours, MOONRISE, MOONSET, MOON_TILT_DEGREES)
}

/// The sunlight's colour (sRGB) at `elevation`, the sine of the sun's elevation: a cubic per
/// channel from a red horizon through a warm 12 degrees to white at noon, flat there.
fn sun_color(elevation: f32) -> Vec3 {
    const HORIZON: Vec3 = Vec3::new(1.0, 0.38, 0.40);
    const E1: Vec3 = Vec3::new(0.0, 0.8804, 1.0065);
    const E2: Vec3 = Vec3::new(0.0, -0.1079, -0.5370);
    const E3: Vec3 = Vec3::new(0.0, -0.2216, 0.0225);
    let e = elevation.max(0.0);
    HORIZON + E1 * e + E2 * e * e + E3 * e * e * e
}

/// The sunlight's colour at noon, where [`sun_color`] peaks (sRGB).
pub const SUN_COLOR_NOON: Vec3 = Vec3::new(1.0, 0.931, 0.892);

/// The moonlight's colour (sRGB): a violet tint, so the scene stays directional after dark.
const MOON_COLOR: Vec3 = Vec3::new(0.514, 0.388, 1.0);

/// Sines of the sun and moon discs' angular radii, as the sky draws them (`nishita_cloud.wgsl`:
/// the sun disc out to a cosine of 0.997, the moon's to 0.99914).
const SUN_DISC_SIN: f32 = 0.0774;
const MOON_DISC_SIN: f32 = 0.0415;

/// How much of the directional light is left while both discs are below the horizon.
const SET_ILLUMINANCE: f32 = 0.01;

/// 0 while a body's disc is entirely below the horizon, 1 once entirely above, ramping across
/// the crossing. `elevation` is the sine of the body's elevation, `disc_sin` of its radius.
fn disc_above_horizon(elevation: f32, disc_sin: f32) -> f32 {
    ((elevation + disc_sin) / (2.0 * disc_sin)).clamp(0.0, 1.0)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn update_directional_light(
    lights: Query<(
        &RendererSceneContext,
        Option<&LightSource>,
        Option<&GlobalLight>,
    )>,
    mut global_light: ResMut<SceneGlobalLight>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<TimeOfDay>,
) {
    // colour and energy driven by the sun's elevation
    let hours = time.elapsed_secs() / 3600.0;
    let sun_direction = sun_direction(hours);
    let elevation = -sun_direction.y;
    let energy = smoothstep(-0.05, 0.3, elevation);
    let dir = MOON_COLOR.lerp(sun_color(elevation), smoothstep(-0.05, 0.05, elevation));

    // the sun lights while it is up and the moon once it has set; the moon's 1500 lux dims to
    // 1% while both discs are below the horizon, ramping as either disc crosses it
    let moon_direction = moon_direction(hours);
    let disc_above = disc_above_horizon(elevation, SUN_DISC_SIN)
        .max(disc_above_horizon(-moon_direction.y, MOON_DISC_SIN));
    let dir_illuminance =
        (1500.0 + smoothstep(0.0, 0.3, elevation) * 5500.0) * SET_ILLUMINANCE.lerp(1.0, disc_above);
    let dir_direction = if elevation > 0.0 {
        sun_direction
    } else {
        moon_direction
    };

    *global_light = SceneGlobalLight {
        source: None,
        dir_color: Color::srgb(dir.x, dir.y, dir.z),
        dir_illuminance,
        dir_direction,
        // sun energy peaks at ~0.7 * full scale; the floor keeps the night sky lit
        sun_illuminance: (energy * 0.7 * 10_000.0).max(1500.0),
        sun_direction,
        ambient_color: Color::WHITE,
        ambient_brightness: 1.0,
        layers: RenderLayers::default(),
    };

    let Ok(player) = player.single() else {
        return;
    };

    let mut apply =
        |parcel: Entity, maybe_light: Option<&LightSource>, maybe_global: Option<&GlobalLight>| {
            global_light.source = Some(parcel);
            if let Some(light) = maybe_light {
                if let Some(color) = light.color {
                    global_light.dir_color = color;
                }
                if let Some(ill) = if light.enabled {
                    light.intensity
                } else {
                    Some(0.0)
                } {
                    global_light.dir_illuminance = ill;
                    global_light.sun_illuminance = ill;
                }
            }

            if let Some(global) = maybe_global {
                if let Some(dir) = global.direction {
                    global_light.dir_direction = dir;
                    global_light.sun_direction = dir;
                }
                if let Some(color) = global.ambient_color {
                    global_light.ambient_color = color;
                }
                if let Some(brightness) = global.ambient_brightness {
                    global_light.ambient_brightness = brightness;
                };
            }
        };

    if let Some(active_parcel) = containing_scene.get_parcel_oow(player) {
        // try and get settings from active parcel scene first
        if let Ok((_, maybe_light, maybe_global)) = lights.get(active_parcel) {
            if maybe_light.is_some() || maybe_global.is_some() {
                apply(active_parcel, maybe_light, maybe_global);
                return;
            }
        }
    }

    // if the primary parcel doesn't specify anything, check any portables
    let mut portable_settings: Option<(&str, Entity, Option<&LightSource>, Option<&GlobalLight>)> =
        None;
    for entity in containing_scene.get_portables(false) {
        if let Ok((ctx, maybe_light, maybe_global)) = lights.get(entity) {
            if maybe_light.is_none() && maybe_global.is_none() {
                continue;
            }

            let apply = match portable_settings {
                None => true,
                Some((existing, ..)) => &*ctx.hash < existing,
            };

            if !apply {
                continue;
            }

            portable_settings = Some((&ctx.hash, entity, maybe_light, maybe_global));
        }
    }

    if let Some((_, portable, maybe_light, maybe_global)) = portable_settings {
        apply(portable, maybe_light, maybe_global);
    }
}

#[derive(Component)]
pub struct LightEntity {
    pub scene: Entity,
    pub enabled: bool,
    pub shadows_enabled: bool,
    pub range: f32,
}

fn update_point_lights(
    q: Query<
        (Entity, &ContainerEntity, &LightSource, Option<&Children>),
        (
            Without<RendererSceneContext>,
            Or<(Changed<LightSource>, With<RetryLightTexture>)>,
        ),
    >,
    scenes: Query<&RendererSceneContext>,
    mut commands: Commands,
    mut removed_points: RemovedComponents<LightSource>,
    children: Query<&Children>,
    child_lights: Query<&LightEntity>,
    mut resolver: TextureResolver,
) {
    for (entity, container, light, maybe_children) in q.iter() {
        commands.entity(entity).remove::<RetryLightTexture>();

        // try and reuse any previous
        let previous_light_entity = maybe_children.and_then(|children| {
            children
                .iter()
                .find(|child| child_lights.get(*child).is_ok())
        });

        let range = light.range.unwrap_or(-1.0);
        let range = if range < 0.0 {
            light.intensity.unwrap_or(16000.0).powf(0.25)
        } else {
            range.min(light.intensity.unwrap_or(16000.0).powf(0.25))
        };

        let mut light_cmds = match previous_light_entity {
            Some(prev) => {
                let mut cmds = commands.entity(prev);
                cmds.insert((
                    LightEntity {
                        scene: container.root,
                        enabled: light.enabled,
                        shadows_enabled: light.shadow.unwrap_or(false),
                        range,
                    },
                    Visibility::Hidden,
                ));
                cmds
            }
            None => {
                let mut cmds = commands.spawn((
                    LightEntity {
                        scene: container.root,
                        enabled: light.enabled,
                        shadows_enabled: light.shadow.unwrap_or(false),
                        range,
                    },
                    // light hidden avatars too
                    RenderLayers::default().union(&PRIMARY_AVATAR_LIGHT_LAYER),
                    Visibility::Hidden,
                ));
                let light_id = cmds.id();
                cmds.commands()
                    .entity(entity)
                    .try_push_children(&[light_id]);
                cmds
            }
        };

        let maybe_light_texture = if let Some(texture) = light.light_texture.as_ref() {
            let Ok(ctx) = scenes.get(container.root) else {
                continue;
            };
            let image = texture
                .tex
                .as_ref()
                .map(|tex| resolver.resolve_texture(ctx, tex));
            let resolved_texture = match image {
                Some(Err(TextureResolveError::SourceNotReady)) => {
                    commands.entity(entity).try_insert(RetryLightTexture);
                    continue;
                }
                None | Some(Err(_)) => None,
                Some(Ok(t)) => Some(t),
            };

            resolved_texture.map(|rt| rt.image)
        } else {
            None
        };

        let lumens = light.intensity.unwrap_or(16000.0) * 4.0 * PI;
        match light.spotlight_angles {
            Some(angles) => {
                light_cmds.try_insert(SpotLight {
                    color: light.color.unwrap_or(Color::WHITE),
                    intensity: lumens,
                    range,
                    radius: 0.0,
                    shadows_enabled: light.shadow.unwrap_or(false),
                    outer_angle: angles.1.clamp(0.0, 179.0) * PI / 360.0,
                    inner_angle: angles.0.clamp(0.0, angles.1.min(179.0)) * PI / 360.0,
                    ..Default::default()
                });

                if let Some(light_texture) = maybe_light_texture {
                    light_cmds.try_insert(SpotLightTexture {
                        image: light_texture,
                    });
                }

                light_cmds.remove::<(PointLight, PointLightTexture)>();
            }
            None => {
                light_cmds.try_insert(PointLight {
                    color: light.color.unwrap_or(Color::WHITE),
                    intensity: lumens,
                    range,
                    radius: 0.0,
                    shadows_enabled: light.shadow.unwrap_or(false),
                    ..Default::default()
                });

                if let Some(light_texture) = maybe_light_texture {
                    light_cmds.try_insert(PointLightTexture {
                        image: light_texture,
                        cubemap_layout: CubemapLayout::CrossHorizontal,
                    });
                }

                light_cmds.remove::<(SpotLight, SpotLightTexture)>();
            }
        };
    }

    for removed_light in removed_points.read() {
        let Ok(children) = children.get(removed_light) else {
            continue;
        };
        for child in children {
            if child_lights.get(*child).is_ok() {
                commands.entity(*child).despawn();
            }
        }
    }
}

#[derive(Component)]
pub struct RetryLightTexture;

fn manage_shadow_casters(
    read_lights: Populated<(Entity, &GlobalTransform, &LightEntity, &ChildOf)>,
    mut write_lights: Query<(
        Option<&mut PointLight>,
        Option<&mut SpotLight>,
        &mut Visibility,
    )>,
    parent_visibility: Query<&InheritedVisibility>,
    player: Single<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    config: Res<AppConfig>,
    mut lights: Local<Vec<(Entity, bool, FloatOrd, bool)>>,
) {
    let (player, player_gt) = player.into_inner();
    let player_t = player_gt.translation();

    let active_scenes = containing_scene.get_area(player, PLAYER_COLLIDER_RADIUS);

    // collect lights
    lights.extend(read_lights.iter().map(|(entity, gt, light, parent)| {
        let enabled = light.enabled
            && parent_visibility
                .get(parent.parent())
                .is_ok_and(|pv| pv.get());
        let importance = if enabled {
            let distance = (gt.translation() - player_t).length();
            let importance = light.range / distance;
            FloatOrd(importance)
        } else {
            FloatOrd(0.0)
        };

        (
            entity,
            active_scenes.contains(&light.scene),
            importance,
            light.shadows_enabled,
        )
    }));
    // sort by scene-active and distance
    lights.sort_by_key(|(_, scene_active, importance, _)| (*scene_active, *importance));
    // enable up to limit
    let mut max_enabled = config.graphics.light_count;
    let mut max_casters = match config.graphics.shadow_settings {
        common::structs::ShadowSetting::Off => 0,
        _ => config.graphics.shadow_caster_count,
    };
    if config.graphics.shadow_distance <= 0. {
        max_casters = 0;
    }
    debug!(
        "found {} lights, enabling up to {} with {} casters",
        lights.len(),
        max_enabled,
        max_casters
    );
    // only write (and trigger change detection) when the target value differs
    for (light, _, importance, shadows_enabled) in lights.drain(..).rev() {
        let (maybe_p, maybe_s, mut vis) = write_lights.get_mut(light).unwrap();

        if importance.0 != 0.0 && max_enabled > 0 {
            vis.set_if_neq(Visibility::Visible);
            max_enabled -= 1;

            let cast = shadows_enabled && max_casters > 0;
            if cast {
                max_casters -= 1;
            }
            if let Some(mut p) = maybe_p {
                if p.shadows_enabled != cast {
                    p.shadows_enabled = cast;
                }
            }
            if let Some(mut s) = maybe_s {
                if s.shadows_enabled != cast {
                    s.shadows_enabled = cast;
                }
            }
        } else {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_follows_unitys_path() {
        let angle = |a: Vec3, b: Vec3| a.angle_between(b).to_degrees();
        // unity-explorer's sun at noon, from its recorded day cycle (bevy axes)
        let noon = Vec3::new(0.109_261_24, -0.987_989_84, -0.109_261_2);
        assert!(angle(sun_direction(12.0), noon) < 0.5);
        for horizon in [SUNRISE, SUNSET] {
            assert!(sun_direction(horizon).y.abs() < 1e-5);
        }
        assert!(sun_direction((SUNRISE + SUNSET) / 2.0).y < -0.9999);
        // below the horizon through the night, and continuous round the whole day
        assert!(sun_direction(0.0).y > 0.0);
        assert!(angle(sun_direction(0.0), sun_direction(24.0)) < 0.01);
        for step in 0..2400 {
            let hours = step as f32 / 100.0;
            assert!(angle(sun_direction(hours), sun_direction(hours + 0.01)) < 0.3);
        }
    }

    #[test]
    fn moon_follows_unitys_path() {
        let angle = |a: Vec3, b: Vec3| a.angle_between(b).to_degrees();
        // unity-explorer's moon light, from its computed celestial path (bevy axes, light travel)
        for (hours, unity) in [
            (0.0, Vec3::new(0.677_1, -0.715_5, 0.172_0)),
            (0.756, Vec3::new(0.454_5, -0.766_0, 0.454_5)),
            (2.0, Vec3::new(-0.024_9, -0.631_9, 0.774_7)),
            (12.0, Vec3::new(-0.546_0, 0.759_0, -0.354_6)),
        ] {
            assert!(angle(moon_direction(hours), unity) < 0.5);
        }
        // rises where the sun rises and sets where the sun sets
        assert!(angle(moon_direction(MOONRISE), sun_direction(SUNRISE)) < 0.01);
        assert!(angle(moon_direction(MOONSET), sun_direction(SUNSET)) < 0.01);
        // above the horizon from moonrise to moonset, and continuous round the whole day
        for step in 1..100 {
            let hours = MOONRISE + (24.0 - MOONRISE + MOONSET) * step as f32 / 100.0;
            assert!(moon_direction(hours).y < 0.0);
        }
        for step in 0..2400 {
            let hours = step as f32 / 100.0;
            assert!(angle(moon_direction(hours), moon_direction(hours + 0.01)) < 0.3);
        }
    }
}
