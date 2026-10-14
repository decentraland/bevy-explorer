//! Translation of `PbParticleSystem` into the fixed-layout uniforms the shaders read, the
//! feature bits that select shader code, and the CPU-side spawner.

use bevy::{prelude::*, render::render_resource::ShaderDefVal};
use dcl_component::proto_components::{
    common::{Color4, ColorRange, FloatRange, Vector3},
    sdk::components::{
        pb_particle_system::{BlendMode, PlaybackState, Shape, SimulationSpace},
        PbParticleSystem,
    },
    Color4DclToBevy,
};

/// Keep in sync with https://github.com/robtfm/movement-scene/blob/main/src/constants.ts
const GRAVITY: Vec3 = Vec3::new(0., -10., 0.);
/// Seconds of continuous emission for a non-looping system.
const CYCLE_DURATION: f32 = 1.0;
/// Byte size of `Particle` in common.wgsl.
pub const PARTICLE_SIZE: u64 = 80;
/// The ring offset and count share a u32 mesh tag.
pub const MAX_CAPACITY: u32 = u16::MAX as u32;
pub const WORKGROUP_SIZE: u32 = 64;

/// Feature bits. Each selects a shader def; the uniform always carries every value.
pub mod features {
    pub const SHAPE_MASK: u32 = 0b11;
    pub const SHAPE_POINT: u32 = 0;
    pub const SHAPE_SPHERE: u32 = 1;
    pub const SHAPE_CONE: u32 = 2;
    pub const SHAPE_BOX: u32 = 3;
    pub const LIMIT_VELOCITY: u32 = 1 << 2;
    pub const ROTATE_OVER_TIME: u32 = 1 << 3;
    pub const WORLD_SPACE: u32 = 1 << 4;
    pub const TEXTURE: u32 = 1 << 5;
    pub const SPRITE_SHEET: u32 = 1 << 6;
    pub const BILLBOARD: u32 = 1 << 7;
    pub const FACE_TRAVEL: u32 = 1 << 8;
    pub const SIZE_OVER_TIME: u32 = 1 << 9;
    pub const COLOR_OVER_TIME: u32 = 1 << 10;

    /// bits the simulation shader varies on
    pub const SIM_MASK: u32 = SHAPE_MASK | LIMIT_VELOCITY | ROTATE_OVER_TIME | WORLD_SPACE;
    /// bits the render shader varies on
    pub const RENDER_MASK: u32 = WORLD_SPACE
        | TEXTURE
        | SPRITE_SHEET
        | BILLBOARD
        | FACE_TRAVEL
        | SIZE_OVER_TIME
        | COLOR_OVER_TIME;
}

pub fn shader_defs(features: u32) -> Vec<ShaderDefVal> {
    let mut defs = Vec::new();
    match features & features::SHAPE_MASK {
        features::SHAPE_SPHERE => defs.push("SHAPE_SPHERE".into()),
        features::SHAPE_CONE => defs.push("SHAPE_CONE".into()),
        features::SHAPE_BOX => defs.push("SHAPE_BOX".into()),
        _ => (),
    }
    for (bit, name) in [
        (features::LIMIT_VELOCITY, "LIMIT_VELOCITY"),
        (features::ROTATE_OVER_TIME, "ROTATE_OVER_TIME"),
        (features::WORLD_SPACE, "WORLD_SPACE"),
        (features::TEXTURE, "TEXTURE"),
        (features::SPRITE_SHEET, "SPRITE_SHEET"),
        (features::BILLBOARD, "BILLBOARD"),
        (features::FACE_TRAVEL, "FACE_TRAVEL"),
        (features::SIZE_OVER_TIME, "SIZE_OVER_TIME"),
        (features::COLOR_OVER_TIME, "COLOR_OVER_TIME"),
    ] {
        if features & bit != 0 {
            defs.push(name.into());
        }
    }
    defs
}

// the ShaderType derive emits a `check` fn per field that dead-code lints when the fields
// are only ever written (see sky_envmap.rs for the same workaround)
mod uniforms {
    #![allow(dead_code)]

    use bevy::{prelude::*, render::render_resource::ShaderType};

    /// Matches `SimParams` in common.wgsl.
    #[derive(ShaderType, Clone, Copy, Debug, Default)]
    pub struct SimParams {
        pub emitter_transform: Mat4,
        pub emitter_rotation: Vec4,
        pub emitter_inv_rotation: Vec4,
        pub initial_rotation: Vec4,
        pub angular_velocity: Vec4,
        pub color_min: Vec4,
        pub color_max: Vec4,
        pub acceleration: Vec3,
        pub lifetime: f32,
        pub box_size: Vec3,
        pub shape_radius: f32,
        pub cone_angle: f32,
        pub speed_min: f32,
        pub speed_max: f32,
        pub size_min: f32,
        pub size_max: f32,
        pub limit_speed: f32,
        pub limit_dampen: f32,
        pub dt: f32,
        pub seed: u32,
        pub first_slot: u32,
        pub slot_count: u32,
        pub spawn_count: u32,
        pub capacity: u32,
        pub _pad: UVec2,
    }

    /// Matches `RenderParams` in common.wgsl.
    #[derive(ShaderType, Clone, Copy, Debug, Default)]
    pub struct RenderParams {
        pub color_over_time_start: Vec4,
        pub color_over_time_end: Vec4,
        pub size_over_time: Vec2,
        pub sprite_tiles: UVec2,
        pub frames_per_second: f32,
        pub capacity: u32,
        pub _pad: Vec2,
    }
}
pub use uniforms::{RenderParams, SimParams};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstConfig {
    pub time: f32,
    pub count: u32,
    /// `None` = repeat forever
    pub cycles: Option<u32>,
    pub interval: f32,
    pub probability: f32,
}

#[derive(Clone, Debug)]
pub struct EmitterConfig {
    pub features: u32,
    pub sim: SimParams,
    pub render: RenderParams,
    pub max_particles: u32,
    pub rate: f32,
    pub looping: bool,
    pub prewarm: bool,
    pub bursts: Vec<BurstConfig>,
    pub playback: PlaybackState,
    pub active: bool,
    pub alpha_mode: AlphaMode,
}

fn linear(color: Option<Color4>) -> Vec4 {
    color
        .map(|color| color.convert_srgba().to_linear().to_vec4())
        .unwrap_or(Vec4::ONE)
}

fn range(range: Option<FloatRange>, default: f32) -> (f32, f32) {
    range
        .map(|range| (range.start, range.end))
        .unwrap_or((default, default))
}

fn color_range(range: Option<&ColorRange>) -> (Vec4, Vec4) {
    range
        .map(|range| (linear(range.start), linear(range.end)))
        .unwrap_or((Vec4::ONE, Vec4::ONE))
}

impl EmitterConfig {
    /// How far any particle can get from the emitter origin, in local units: the shape
    /// extent plus the furthest reach over a lifetime, plus half the largest quad.
    pub fn bound_radius(&self) -> f32 {
        let sim = &self.sim;
        let extent = match self.features & features::SHAPE_MASK {
            features::SHAPE_BOX => sim.box_size.abs().max_element() * 0.5,
            _ => sim.shape_radius,
        };
        let lifetime = sim.lifetime;
        let reach = sim.speed_max.max(sim.speed_min).abs() * lifetime
            + 0.5 * sim.acceleration.length() * lifetime * lifetime;
        let size = sim.size_max.max(sim.size_min).abs()
            * self
                .render
                .size_over_time
                .x
                .max(self.render.size_over_time.y)
                .abs()
                .max(1.);
        extent + reach + size * 0.5
    }

    pub fn from_proto(ps: &PbParticleSystem) -> Self {
        let mut features = 0;

        let (shape, shape_radius, cone_angle, box_size) = match ps.shape.as_ref() {
            None | Some(Shape::Point(_)) => (features::SHAPE_POINT, 0., 0., Vec3::ONE),
            Some(Shape::Sphere(sphere)) => (
                features::SHAPE_SPHERE,
                sphere.radius.unwrap_or(1.),
                0.,
                Vec3::ONE,
            ),
            Some(Shape::Cone(cone)) => (
                features::SHAPE_CONE,
                cone.radius.unwrap_or(1.),
                cone.angle.unwrap_or(25.).to_radians(),
                Vec3::ONE,
            ),
            Some(Shape::Box(r#box)) => (
                features::SHAPE_BOX,
                0.,
                0.,
                r#box
                    .size
                    .as_ref()
                    .map(Vector3::world_vec_to_vec3)
                    .unwrap_or(Vec3::ONE),
            ),
        };
        features |= shape;

        let (limit_speed, limit_dampen) = match ps.limit_velocity.as_ref() {
            Some(limit) => {
                features |= features::LIMIT_VELOCITY;
                (limit.speed, limit.dampen.unwrap_or(1.).clamp(0., 1.))
            }
            None => (0., 0.),
        };

        let angular_velocity = ps
            .rotation_over_time
            .map(|quat| {
                let (axis, angle) = quat.to_bevy_normalized().to_axis_angle();
                axis * angle
            })
            .unwrap_or(Vec3::ZERO);
        if angular_velocity != Vec3::ZERO {
            features |= features::ROTATE_OVER_TIME;
        }

        if ps.simulation_space() == SimulationSpace::PssWorld {
            features |= features::WORLD_SPACE;
        }
        if ps.billboard.unwrap_or(true) {
            features |= features::BILLBOARD;
        }
        if ps.face_travel_direction.unwrap_or(false) {
            features |= features::FACE_TRAVEL;
        }

        let size_over_time = range(ps.size_over_time, 1.);
        if size_over_time != (1., 1.) {
            features |= features::SIZE_OVER_TIME;
        }
        let color_over_time = color_range(ps.color_over_time.as_ref());
        if color_over_time != (Vec4::ONE, Vec4::ONE) {
            features |= features::COLOR_OVER_TIME;
        }

        let (sprite_tiles, frames_per_second) = match ps.sprite_sheet.as_ref() {
            Some(sheet) => {
                features |= features::SPRITE_SHEET;
                (
                    UVec2::new(sheet.tiles_x.max(1), sheet.tiles_y.max(1)),
                    sheet.frames_per_second.unwrap_or(30.),
                )
            }
            None => (UVec2::ONE, 0.),
        };

        let (speed_min, speed_max) = range(ps.initial_velocity_speed, 1.);
        let (size_min, size_max) = range(ps.initial_size, 1.);
        let (color_min, color_max) = color_range(ps.initial_color.as_ref());
        let additional_force = ps
            .additional_force
            .as_ref()
            .map(Vector3::world_vec_to_vec3)
            .unwrap_or(Vec3::ZERO);

        let sim = SimParams {
            emitter_transform: Mat4::IDENTITY,
            emitter_rotation: Vec4::from(Quat::IDENTITY),
            emitter_inv_rotation: Vec4::from(Quat::IDENTITY),
            initial_rotation: Vec4::from(
                ps.initial_rotation
                    .map(|quat| quat.to_bevy_normalized())
                    .unwrap_or_default(),
            ),
            angular_velocity: angular_velocity.extend(0.),
            color_min,
            color_max,
            acceleration: GRAVITY * ps.gravity.unwrap_or(0.) + additional_force,
            lifetime: ps.lifetime.unwrap_or(5.).max(f32::EPSILON),
            box_size,
            shape_radius,
            cone_angle,
            speed_min,
            speed_max,
            size_min,
            size_max,
            limit_speed,
            limit_dampen,
            dt: 0.,
            seed: 0,
            first_slot: 0,
            slot_count: 0,
            spawn_count: 0,
            capacity: 0,
            _pad: UVec2::ZERO,
        };

        let render = RenderParams {
            color_over_time_start: color_over_time.0,
            color_over_time_end: color_over_time.1,
            size_over_time: Vec2::new(size_over_time.0, size_over_time.1),
            sprite_tiles,
            frames_per_second,
            capacity: 0,
            _pad: Vec2::ZERO,
        };

        let bursts = ps
            .bursts
            .as_ref()
            .map(|bursts| {
                bursts
                    .values
                    .iter()
                    .map(|burst| {
                        let interval = burst.interval.unwrap_or(0.01);
                        BurstConfig {
                            time: burst.time.max(0.),
                            count: burst.count,
                            cycles: match burst.cycles.unwrap_or(1) {
                                0 => None,
                                cycles => Some(cycles.max(1) as u32),
                            },
                            // repeated bursts need a positive interval or they never end
                            interval: if interval.is_finite() {
                                interval.max(0.001)
                            } else {
                                0.001
                            },
                            probability: burst.probability.unwrap_or(1.).clamp(0., 1.),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            features,
            sim,
            render,
            max_particles: ps.max_particles.unwrap_or(1000),
            rate: ps.rate.unwrap_or(10.).max(0.),
            looping: ps.r#loop.unwrap_or(true),
            prewarm: ps.prewarm.unwrap_or(false),
            bursts,
            playback: ps.playback_state(),
            active: ps.active.unwrap_or(true),
            alpha_mode: match ps.blend_mode() {
                BlendMode::PsbAlpha => AlphaMode::Blend,
                BlendMode::PsbAdd => AlphaMode::Add,
                BlendMode::PsbMultiply => AlphaMode::Multiply,
            },
        }
    }
}

#[derive(Clone, Debug)]
struct BurstState {
    config: BurstConfig,
    next: f32,
    remaining: Option<u32>,
}

/// Decides how many particles to spawn each frame: the continuous rate plus bursts.
#[derive(Clone, Debug, Default)]
pub struct Spawner {
    time: f32,
    accumulator: f32,
    bursts: Vec<BurstState>,
}

impl Spawner {
    pub fn new(config: &EmitterConfig) -> Self {
        let mut spawner = Self::default();
        spawner.sync_bursts(config);
        spawner
    }

    pub fn reset(&mut self, config: &EmitterConfig) {
        *self = Self::new(config);
    }

    /// Restart the burst schedule when the burst list changes; running bursts are kept.
    pub fn sync_bursts(&mut self, config: &EmitterConfig) {
        if self
            .bursts
            .iter()
            .map(|burst| burst.config)
            .eq(config.bursts.iter().copied())
        {
            return;
        }
        self.bursts = config
            .bursts
            .iter()
            .map(|config| BurstState {
                config: *config,
                next: self.time.max(config.time),
                remaining: config.cycles,
            })
            .collect();
    }

    pub fn tick(&mut self, dt: f32, config: &EmitterConfig) -> u32 {
        let start = self.time;
        self.time += dt;
        let mut count = 0;

        let emitting = if config.looping {
            dt
        } else {
            (CYCLE_DURATION - start).clamp(0., dt)
        };
        self.accumulator += config.rate * emitting;
        let whole = self.accumulator.floor();
        self.accumulator -= whole;
        count += whole as u32;

        for burst in &mut self.bursts {
            // bound the catch-up after a long stall
            let mut guard = 0;
            while burst.remaining != Some(0) && self.time >= burst.next && guard < 100 {
                guard += 1;
                if burst.config.probability >= 1.
                    || rand::random::<f32>() < burst.config.probability
                {
                    count += burst.config.count;
                }
                burst.next += burst.config.interval;
                if let Some(remaining) = burst.remaining.as_mut() {
                    *remaining -= 1;
                }
            }
        }

        count
    }
}

/// The live particles as a FIFO ring over the buffer: the CPU decides spawn counts and
/// knows every lifetime, so it knows which slots are live without reading the GPU back.
#[derive(Clone, Debug, Default)]
pub struct ParticleRing {
    /// simulation time, the sum of the dts sent to the shader
    time: f32,
    /// next slot to spawn into
    head: u32,
    /// live slots ending at `head`
    alive: u32,
    /// (death time, count) in spawn order
    events: std::collections::VecDeque<(f32, u32)>,
}

impl ParticleRing {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn alive(&self) -> u32 {
        self.alive
    }

    /// The oldest live slot.
    pub fn first(&self, capacity: u32) -> u32 {
        let capacity = capacity.max(1);
        (self.head + capacity - self.alive % capacity) % capacity
    }

    /// Advance one step: retire expired particles, then spawn up to `spawn` new ones.
    /// Returns (first live slot, live count, spawned count).
    pub fn step(&mut self, dt: f32, spawn: u32, lifetime: f32, capacity: u32) -> (u32, u32, u32) {
        self.time += dt;
        while let Some(&(death, count)) = self.events.front() {
            if death > self.time {
                break;
            }
            self.events.pop_front();
            self.alive = self.alive.saturating_sub(count);
        }
        // over capacity, drop the new ones (Unity stops emitting at max)
        let spawn = spawn.min(capacity.saturating_sub(self.alive));
        if spawn > 0 {
            self.events.push_back((self.time + lifetime, spawn));
            self.head = (self.head + spawn) % capacity.max(1);
            self.alive += spawn;
        }
        (self.first(capacity), self.alive, spawn)
    }
}
