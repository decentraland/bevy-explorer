mod random_color_modifier;
mod set_position_modifier;
mod speed_dampen;

use bevy::{math::bounding::Aabb3d, prelude::*};
use bevy_hanabi::{
    AccelModifier, Attribute, EffectAsset, ExprHandle, ExprWriter, Gradient, HanabiPlugin,
    OrientMode, OrientModifier, ParticleEffect, ScalarType, SetAttributeModifier,
    SetPositionCone3dModifier, SetPositionSphereModifier, SetVelocitySphereModifier,
    SizeOverLifetimeModifier, SpawnerSettings,
};
use dcl_component::{
    proto_components::{
        common::{Color4, ColorRange, FloatRange, Vector3},
        sdk::components::{pb_particle_system::Shape, PbParticleSystem},
        Color4DclToBevy,
    },
    ComponentPosition, SceneComponentId,
};
use scene_runner::update_world::AddCrdtInterfaceExt;

use crate::{
    plugin::{
        random_color_modifier::RandomColorModifier, set_position_modifier::SetPositionModifier,
        speed_dampen::SpeedDampenModifier,
    },
    ParticleSystem,
};

const MIN_SPHERE_RADIUS: f32 = 1. / 128.;
/// Keep in sync with https://github.com/robtfm/movement-scene/blob/main/src/constants.ts
const GRAVITY: Vec3 = Vec3::new(0., -10., 0.);

const ROTATION_ATTR: Attribute = Attribute::F32_0;

macro_rules! set {
    ($effect:expr, $stage:ident, $value:expr) => {
        $effect = $effect.$stage($value);
    };
}

pub struct ParticleSystemPlugin;

impl Plugin for ParticleSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HanabiPlugin);

        app.add_crdt_lww_component::<PbParticleSystem, ParticleSystem>(
            SceneComponentId::PARTICLE_SYSTEM,
            ComponentPosition::EntityOnly,
        );

        app.add_observer(particle_system_on_insert);
        app.add_observer(particle_system_on_remove);
    }
}

fn particle_system_on_insert(
    trigger: Trigger<OnInsert, ParticleSystem>,
    mut commands: Commands,
    particle_systems: Query<&ParticleSystem>,
    mut effect_assets: ResMut<Assets<EffectAsset>>,
) {
    let entity = trigger.target();
    let Ok(particle_system) = particle_systems.get(entity) else {
        unreachable!("Infallible query");
    };

    let active = particle_system.active.unwrap_or(true);
    let rate = particle_system.rate.unwrap_or(10.);
    let max_particles = particle_system.max_particles.unwrap_or(1000);
    let lifetime = particle_system.lifetime.unwrap_or(5.);
    // TODO bursts
    let gravity = particle_system.gravity.unwrap_or(0.);
    let additional_force = particle_system
        .additional_force
        .as_ref()
        .map(Vector3::abs_vec_to_vec3)
        .unwrap_or(Vec3::ZERO);
    let initial_velocity_speed = particle_system
        .initial_velocity_speed
        .unwrap_or(FloatRange { start: 1., end: 1. });
    let limit_velocity = particle_system.limit_velocity.as_ref();
    let billboard = particle_system.billboard.unwrap_or(true);
    let initial_size = particle_system
        .initial_size
        .unwrap_or(FloatRange { start: 1., end: 1. });
    let size_over_lifetime = particle_system
        .size_over_time
        .unwrap_or(FloatRange { start: 1., end: 1. });
    // TODO initial_rotation
    // TODO rotation_over_time
    // TODO face_travel_direction
    let initial_color = particle_system.initial_color.unwrap_or(ColorRange {
        start: Some(Color4 {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        }),
        end: Some(Color4 {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        }),
    });

    let writer = ExprWriter::new();

    // Modifiers
    let init_position = make_position(particle_system.shape.as_ref(), &writer);
    let init_rotation = SetAttributeModifier::new(
        ROTATION_ATTR,
        (writer.rand(ScalarType::Float) * writer.lit(std::f32::consts::TAU)).expr(),
    );
    let init_size = SetAttributeModifier::new(
        Attribute::SIZE,
        random_lerp(&writer, initial_size.start, initial_size.end),
    );
    let init_velocity = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: random_lerp(
            &writer,
            initial_velocity_speed.start,
            initial_velocity_speed.end,
        ),
    };
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.).expr());
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(lifetime).expr());
    let init_color = RandomColorModifier {
        start: writer
            .lit(
                initial_color
                    .start
                    .map(|color| color.convert_srgba())
                    .unwrap_or(Color::WHITE)
                    .to_linear()
                    .to_vec4(),
            )
            .expr(),
        end: writer
            .lit(
                initial_color
                    .end
                    .map(|color| color.convert_srgba())
                    .unwrap_or(Color::WHITE)
                    .to_linear()
                    .to_vec4(),
            )
            .expr(),
    };

    let update_accel = AccelModifier::new(
        (writer.lit(GRAVITY) * writer.lit(Vec3::new(1., gravity, 1.))
            + writer.lit(additional_force))
        .expr(),
    );
    let update_clamp_velocity = limit_velocity.map(|limit_velocity| SpeedDampenModifier {
        max_speed: writer.lit(limit_velocity.speed).expr(),
        dampen: writer
            .lit(limit_velocity.dampen.unwrap_or(1.).clamp(0., 1.))
            .expr(),
    });

    let render_size_over_lifetime = SizeOverLifetimeModifier {
        gradient: Gradient::from_keys([
            (0., Vec3::splat(size_over_lifetime.start)),
            (1., Vec3::splat(size_over_lifetime.end)),
        ]),
        screen_space_size: false,
    };
    let render_billboard = OrientModifier {
        mode: OrientMode::FaceCameraPosition,
        rotation: Some(writer.attr(ROTATION_ATTR).expr()),
    };

    let module = writer.finish();

    let mut effect_asset = EffectAsset::new(
        max_particles,
        SpawnerSettings::rate(rate.into()).with_starts_active(active),
        module,
    );

    set!(effect_asset, init, init_position);
    set!(effect_asset, init, init_rotation);
    set!(effect_asset, init, init_size);
    set!(effect_asset, init, init_velocity);
    set!(effect_asset, init, init_age);
    set!(effect_asset, init, init_lifetime);
    set!(effect_asset, init, init_color);

    set!(effect_asset, update, update_accel);
    if let Some(update_clamp_velocity) = update_clamp_velocity {
        set!(effect_asset, update, update_clamp_velocity);
    }

    set!(effect_asset, render, render_size_over_lifetime);
    if billboard {
        set!(effect_asset, render, render_billboard);
    }

    let handle = effect_assets.add(effect_asset);

    commands.entity(entity).insert(ParticleEffect::new(handle));
}

fn particle_system_on_remove(trigger: Trigger<OnRemove, ParticleSystem>, mut commands: Commands) {
    // On replace ParticleEffect will be replaced with a new value
    // On despawn ParticleEffect will cease to exist anyways
    commands
        .entity(trigger.target())
        .try_remove::<ParticleEffect>();
}

fn make_position(shape: Option<&Shape>, writer: &ExprWriter) -> SetPositionModifier {
    match shape {
        None | Some(Shape::Point(_)) => SetPositionModifier::Sphere(SetPositionSphereModifier {
            center: writer.lit(Vec3::ZERO).expr(),
            radius: writer.lit(MIN_SPHERE_RADIUS).expr(),
            dimension: bevy_hanabi::ShapeDimension::Volume,
        }),
        Some(Shape::Sphere(sphere)) => SetPositionModifier::Sphere(SetPositionSphereModifier {
            center: writer.lit(Vec3::ZERO).expr(),
            radius: writer
                .lit(sphere.radius.unwrap_or(1.).max(MIN_SPHERE_RADIUS))
                .expr(),
            dimension: bevy_hanabi::ShapeDimension::Volume,
        }),
        Some(Shape::Box(r#box)) => {
            // bevy_hanabi does not have a box spawner, faking it with a sphere
            SetPositionModifier::Sphere(SetPositionSphereModifier {
                center: writer.lit(Vec3::ZERO).expr(),
                radius: writer
                    .lit(
                        Aabb3d::new(
                            Vec3::ZERO,
                            r#box
                                .size
                                .as_ref()
                                .map(Vector3::abs_vec_to_vec3)
                                .unwrap_or(Vec3::ONE)
                                / 2.,
                        )
                        .bounding_sphere()
                        .radius(),
                    )
                    .expr(),
                dimension: bevy_hanabi::ShapeDimension::Volume,
            })
        }
        Some(Shape::Cone(cone)) => SetPositionModifier::Cone3d(SetPositionCone3dModifier {
            base_radius: writer.lit(cone.radius.unwrap_or(1.)).expr(),
            height: writer.lit(1.).expr(),
            top_radius: writer.lit(cone.radius.unwrap_or(1.)).expr(),
            dimension: bevy_hanabi::ShapeDimension::Surface,
        }),
    }
}

fn random_lerp(writer: &ExprWriter, start: f32, end: f32) -> ExprHandle {
    let expr =
        writer.lit(start) + (writer.lit(end) - writer.lit(start)) * writer.rand(ScalarType::Float);
    expr.expr()
}
