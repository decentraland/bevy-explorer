use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{math::FloatOrd, prelude::*, render::view::RenderLayers};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser, PRIMARY_AVATAR_LIGHT_LAYER},
    util::TryPushChildrenEx,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{PbGlobalLight, PbLight, PbSpotlight},
    },
    SceneComponentId,
};
use visuals::SceneGlobalLight;

use crate::{renderer_context::RendererSceneContext, ContainerEntity, ContainingScene};

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
            ComponentPosition::RootOnly,
        );
        app.add_systems(
            Update,
            (
                update_directional_light,
                update_point_lights,
                manage_shadow_casters,
            )
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component, Debug)]
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
    let t = ((TAU * 0.15 + time.elapsed_seconds_wrapped() / 20.0) % TAU) * 0.6 - TAU * 0.05;

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
    for entity in containing_scene.get_portables(false) {
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

#[derive(Component)]
pub struct LightEntity {
    pub scene: Entity,
}

fn update_point_lights(
    q: Query<
        (
            Entity,
            &ContainerEntity,
            &Light,
            Option<&SpotlightAngles>,
            Option<&Children>,
        ),
        (
            Without<RendererSceneContext>,
            Or<(Changed<Light>, Changed<SpotlightAngles>)>,
        ),
    >,
    mut commands: Commands,
    mut removed_points: RemovedComponents<Light>,
    children: Query<&Children>,
    child_lights: Query<&LightEntity>,
) {
    for (entity, container, light, angles, maybe_children) in q.iter() {
        // despawn any previous
        if let Some(children) = maybe_children {
            for child in children.iter() {
                if child_lights.get(*child).is_ok() {
                    commands.entity(*child).despawn_recursive();
                }
            }
        }

        let lumens = if light.enabled {
            light.illuminance.unwrap_or(10000.0) * 4.0 * PI
        } else {
            0.0
        };
        let range = light.illuminance.unwrap_or(10000.0).powf(0.25);
        let mut light = match angles {
            Some(angles) => commands.spawn(SpotLightBundle {
                spot_light: SpotLight {
                    color: light.color.unwrap_or(Color::WHITE),
                    intensity: lumens,
                    range,
                    radius: 0.0,
                    shadows_enabled: light.shadows.unwrap_or(false),
                    outer_angle: angles.outer_angle,
                    inner_angle: angles.inner_angle,
                    ..Default::default()
                },
                ..Default::default()
            }),
            None => commands.spawn(PointLightBundle {
                point_light: PointLight {
                    color: light.color.unwrap_or(Color::WHITE),
                    intensity: lumens,
                    range,
                    radius: 0.0,
                    shadows_enabled: light.shadows.unwrap_or(false),
                    ..Default::default()
                },
                ..Default::default()
            }),
        };

        let light_id = light
            .insert((
                LightEntity {
                    scene: container.root,
                },
                // light hidden avatars too
                RenderLayers::default().union(&PRIMARY_AVATAR_LIGHT_LAYER),
            ))
            .id();
        commands.entity(entity).try_push_children(&[light_id]);
    }

    for removed_light in removed_points.read() {
        let Ok(children) = children.get(removed_light) else {
            continue;
        };
        for child in children {
            if child_lights.get(*child).is_ok() {
                commands.entity(*child).despawn_recursive();
            }
        }
    }
}

fn manage_shadow_casters(
    mut q: Query<
        (
            Entity,
            &GlobalTransform,
            &LightEntity,
            Option<&mut PointLight>,
            Option<&mut SpotLight>,
        ),
        Or<(With<PointLight>, With<SpotLight>)>,
    >,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    config: Res<AppConfig>,
    mut lights: Local<Vec<(Entity, bool, FloatOrd, bool)>>,
) {
    let Ok((player, player_gt)) = player.get_single() else {
        return;
    };
    let player_t = player_gt.translation();

    let active_scenes = containing_scene.get_area(player, PLAYER_COLLIDER_RADIUS);

    // collect lights
    lights.extend(q.iter().map(|(entity, gt, container, maybe_p, maybe_s)| {
        (
            entity,
            active_scenes.contains(&container.scene),
            FloatOrd(gt.translation().distance_squared(player_t)),
            maybe_p
                .map(|p| p.shadows_enabled)
                .unwrap_or_else(|| maybe_s.unwrap().shadows_enabled),
        )
    }));
    // sort by scene-active and distance
    lights.sort_by_key(|(_, scene_active, distance, _)| (*scene_active, *distance));
    // enable up to limit
    let max_casters = match config.graphics.shadow_settings {
        common::structs::ShadowSetting::Off => 0,
        _ => config.graphics.shadow_caster_count,
    };
    debug!(
        "found {} lights, enabling up to {}",
        lights.len(),
        max_casters
    );
    let mut iter = lights.drain(..);
    for (light, _, _, enabled) in iter.by_ref().take(max_casters) {
        if !enabled {
            let (_, _, _, maybe_p, maybe_s) = q.get_mut(light).unwrap();
            if let Some(mut p) = maybe_p {
                p.shadows_enabled = true;
            }
            if let Some(mut s) = maybe_s {
                s.shadows_enabled = true;
            }
        }
    }
    // disable over limit
    for (light, _, _, enabled) in iter {
        if enabled {
            let (_, _, _, maybe_p, maybe_s) = q.get_mut(light).unwrap();
            if let Some(mut p) = maybe_p {
                p.shadows_enabled = false;
            }
            if let Some(mut s) = maybe_s {
                s.shadows_enabled = false;
            }
        }
    }
}
