use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{
    pbr::CubemapVisibleEntities,
    prelude::*,
    render::{
        primitives::{CubemapFrusta, Frustum},
        view::VisibleEntities,
    },
};
use common::{sets::SceneSets, structs::PrimaryUser};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{PbGlobalLight, PbLight, PbSpotlight},
    },
    SceneComponentId,
};
use visuals::SceneGlobalLight;

use crate::{renderer_context::RendererSceneContext, ContainingScene};

use super::AddCrdtInterfaceExt;

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbLight, Light>(
            SceneComponentId::LIGHT,
            ComponentPosition::Any,
        );
        app.add_crdt_lww_component::<PbSpotlight, SpotlightAngles>(
            SceneComponentId::SPOTLIGHT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbGlobalLight, GlobalLight>(
            SceneComponentId::GLOBAL_LIGHT,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (update_directional_light, update_point_lights).in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component)]
pub struct Light {
    pub enabled: bool,
    pub illuminance: Option<f32>,
    pub shadows: Option<bool>,
    pub color: Option<Color>,
}

impl From<PbLight> for Light {
    fn from(value: PbLight) -> Self {
        Self {
            enabled: value.enabled.unwrap_or(true),
            illuminance: value.illuminance,
            shadows: value.shadows,
            color: value.color.map(Into::into),
        }
    }
}

#[derive(Component)]
pub struct SpotlightAngles {
    pub inner_angle: f32,
    pub outer_angle: f32,
}

impl From<PbSpotlight> for SpotlightAngles {
    fn from(value: PbSpotlight) -> Self {
        Self {
            inner_angle: value.inner_angle.unwrap_or(value.angle).min(value.angle),
            outer_angle: value.angle,
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
            ambient_color: value.ambient_color.map(Into::into),
            ambient_brightness: value.ambient_brightness,
        }
    }
}

fn update_directional_light(
    lights: Query<(&RendererSceneContext, Option<&Light>, Option<&GlobalLight>)>,
    mut global_light: ResMut<SceneGlobalLight>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<Time>,
) {
    // reset to default
    let t = ((TAU * 0.15 + time.elapsed_seconds_wrapped() / 200.0) % TAU) * 0.6 - TAU * 0.05;

    *global_light = SceneGlobalLight {
        source: None,
        dir_color: Color::srgb(1.0, 1.0, 0.7),
        dir_illuminance: t.sin().max(0.0).powf(2.0) * 10_000.0,
        dir_direction: Quat::from_euler(EulerRot::YXZ, FRAC_PI_2 * 0.8, -t, 0.0) * Vec3::NEG_Z,
        ambient_color: Color::srgb(0.85, 0.85, 1.0),
        ambient_brightness: 1.0,
    };

    let Ok(player) = player.get_single() else {
        return;
    };

    let mut apply =
        |parcel: Entity, maybe_light: Option<&Light>, maybe_global: Option<&GlobalLight>| {
            global_light.source = Some(parcel);
            if let Some(light) = maybe_light {
                if let Some(color) = light.color {
                    global_light.dir_color = color;
                }
                if let Some(ill) = if light.enabled {
                    light.illuminance
                } else {
                    Some(0.0)
                } {
                    global_light.dir_illuminance = ill;
                }
            }

            if let Some(global) = maybe_global {
                if let Some(dir) = global.direction {
                    global_light.dir_direction = dir;
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
    let mut portable_settings: Option<(&String, Entity, Option<&Light>, Option<&GlobalLight>)> =
        None;
    for entity in containing_scene.get_portables() {
        if let Ok((ctx, maybe_light, maybe_global)) = lights.get(entity) {
            if maybe_light.is_none() && maybe_global.is_none() {
                continue;
            }

            let apply = match portable_settings {
                None => true,
                Some((existing, ..)) => &ctx.hash < existing,
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

fn update_point_lights(
    q: Query<
        (
            Entity,
            &Light,
            Option<&SpotlightAngles>,
            Option<&PointLight>,
            Option<&SpotLight>,
        ),
        (
            Without<RendererSceneContext>,
            Or<(Changed<Light>, Changed<SpotlightAngles>)>,
        ),
    >,
    mut commands: Commands,
    mut removed_spots: RemovedComponents<SpotlightAngles>,
    mut removed_points: RemovedComponents<Light>,
) {
    for (entity, light, angles, maybe_point, maybe_spot) in q.iter() {
        let lumens = if light.enabled {
            light.illuminance.unwrap_or(10000.0) * 4.0 * PI
        } else {
            0.0
        };
        // 10 lumens cutoff
        let range = light.illuminance.unwrap_or(10000.0).sqrt();
        let Some(mut commands) = commands.get_entity(entity) else {
            continue;
        };
        match angles {
            Some(angles) => {
                if maybe_point.is_some() {
                    commands.remove::<(PointLight, CubemapVisibleEntities, CubemapFrusta)>();
                }

                commands.try_insert((
                    SpotLight {
                        color: light.color.unwrap_or(Color::WHITE),
                        intensity: lumens,
                        range,
                        radius: 0.0,
                        shadows_enabled: light.shadows.unwrap_or(false),
                        outer_angle: angles.outer_angle,
                        inner_angle: angles.inner_angle,
                        ..Default::default()
                    },
                    VisibleEntities::default(),
                    Frustum::default(),
                ));
            }
            None => {
                if maybe_spot.is_some() {
                    commands.remove::<(SpotLight, VisibleEntities, Frustum)>();
                }

                commands.insert((
                    PointLight {
                        color: light.color.unwrap_or(Color::WHITE),
                        intensity: lumens,
                        range,
                        radius: 0.0,
                        shadows_enabled: light.shadows.unwrap_or(false),
                        ..Default::default()
                    },
                    CubemapVisibleEntities::default(),
                    CubemapFrusta::default(),
                ));
            }
        }
    }

    for removed_spot in removed_spots.read() {
        if let Some(mut commands) = commands.get_entity(removed_spot) {
            commands.remove::<(SpotLight, VisibleEntities, Frustum)>();
        }
    }

    for removed_point in removed_points.read() {
        if let Some(mut commands) = commands.get_entity(removed_point) {
            commands.remove::<(PointLight, CubemapVisibleEntities, CubemapFrusta)>();
        }
    }
}
