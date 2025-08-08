#![allow(clippy::new_without_default)]
use std::f32::consts::TAU;

use bevy::{
    log::debug,
    math::{IVec2, Vec2, Vec3, Vec3Swizzles},
};
use common::structs::MoveKind;
use modular_bitfield::specifiers::*;
use modular_bitfield::{bitfield, BitfieldSpecifier, Specifier};
use num_traits::AsPrimitive;

#[inline(always)]
fn quantize<T: Specifier>(uncompressed: f32, min: f32, max: f32) -> T::InOut
where
    <T as Specifier>::InOut: TryFrom<u32>,
    <<T as Specifier>::InOut as TryFrom<u32>>::Error: core::fmt::Debug,
{
    let max_step: u32 = (1 << T::BITS) - 1;
    let normalized_value: f32 = (uncompressed - min) / (max - min);
    ((normalized_value.clamp(0.0, 1.0) * max_step as f32) as u32)
        .try_into()
        .unwrap()
}

#[inline(always)]
fn dequantize<T: Specifier>(compressed: <T as Specifier>::InOut, min: f32, max: f32) -> f32
where
    <T as Specifier>::InOut: AsPrimitive<f32>,
{
    let max_step: f32 = (1 << T::BITS) as f32 - 1.0;
    let normalized_value: f32 = (compressed.as_()) / max_step;
    (normalized_value * (max - min)) + min
}

#[derive(BitfieldSpecifier, Default, Debug)]
pub enum Kind {
    #[default]
    Idle,
    Walk,
    Jog,
    Run,
}

#[derive(BitfieldSpecifier, Default, Debug)]
#[bits = 2]
pub enum VelocityTier {
    #[default]
    None = 0,
    Slow = 1,
    Med = 2,
    Fast = 3,
}

mod decl {
    // temporary scope for unused_parens in bitfield macro
    #![allow(unused_parens)]
    use modular_bitfield::bitfield;
    use modular_bitfield::specifiers::*;

    #[bitfield]
    #[derive(Debug)]
    pub struct Temporal {
        pub timestamp: B15,
        pub movement_kind: super::Kind,
        pub sliding: bool,
        pub stunned: bool,
        pub grounded: bool,
        pub jump: bool,
        pub long_jump: bool,
        pub falling: bool,
        pub long_falling: bool,
        pub rotation_y: B6,
        pub velocity_tier: super::VelocityTier,
    }
}
pub use decl::*;

impl Temporal {
    const TIMESTAMP_BITS: u32 = 15;
    const TIMESTAMP_QUANTUM: f32 = 0.02;
    pub const TIMESTAMP_MAX: f32 = (1 << Self::TIMESTAMP_BITS) as f32 * Self::TIMESTAMP_QUANTUM;

    pub fn from_parts(
        time: f64,
        falling: bool,
        rotation: f32,
        tier: VelocityTier,
        kind: MoveKind,
        grounded: bool,
    ) -> Self {
        let mut val = Self::new()
            .with_timestamp_f32(time as f32)
            .with_falling(falling)
            .with_rotation_f32(rotation)
            .with_velocity_tier(tier)
            .with_movement_kind(Kind::Idle)
            .with_grounded(grounded);

        match kind {
            MoveKind::Idle => (),
            MoveKind::Walk => val.set_movement_kind(Kind::Walk),
            MoveKind::Jog => val.set_movement_kind(Kind::Jog),
            MoveKind::Run => val.set_movement_kind(Kind::Run),
            MoveKind::Jump => val.set_jump(true),
            MoveKind::Falling => val.set_falling(true),
            MoveKind::LongFalling => val.set_long_falling(true),
            MoveKind::Emote => (),
        }

        val
    }

    pub fn with_timestamp_f32(self, time: f32) -> Self {
        self.with_timestamp(quantize::<B15>(
            time % Self::TIMESTAMP_MAX,
            0.0,
            Self::TIMESTAMP_MAX,
        ))
    }

    // in the range 0..Self::TIMESTAMP_MAX
    pub fn timestamp_f32(&self) -> f32 {
        dequantize::<B15>(self.timestamp(), 0.0, Self::TIMESTAMP_MAX)
    }

    // radians
    pub fn with_rotation_f32(self, rotation: f32) -> Self {
        self.with_rotation_y(quantize::<B6>(((-rotation % TAU) + TAU) % TAU, 0.0, TAU))
    }

    // radians
    pub fn rotation_f32(&self) -> f32 {
        TAU - dequantize::<B6>(self.rotation_y(), 0.0, TAU)
    }
}

#[bitfield]
pub struct MovementSlow {
    parcel_index: B17,
    position_x: B10,
    position_z: B10,
    position_y: B13,
    velocity_x_sign: bool,
    velocity_x: B3,
    velocity_y_sign: bool,
    velocity_y: B3,
    velocity_z_sign: bool,
    velocity_z: B3,
    #[allow(dead_code)]
    unused: B2,
}

#[bitfield]
pub struct MovementFast {
    parcel_index: B17,
    position_x: B8,
    position_z: B8,
    position_y: B13,
    velocity_x_sign: bool,
    velocity_x: B5,
    velocity_y_sign: bool,
    velocity_y: B5,
    velocity_z_sign: bool,
    velocity_z: B5,
}

pub enum Movement {
    Slow(MovementSlow),
    Med(MovementFast),
    Fast(MovementFast),
}

impl Movement {
    pub fn new(position: Vec3, velocity: Vec3, map_min: IVec2, map_max: IVec2) -> Self {
        if map_min == IVec2::MAX {
            return Self::Slow(MovementSlow::new());
        }

        let width = map_max
            .x
            .saturating_sub(map_min.x)
            .saturating_add(2 * 2 + 1);
        let parcel = (position.xz() * Vec2::new(1.0, -1.0) / 16.0)
            .floor()
            .as_ivec2();

        let parcel_index = parcel
            .x
            .saturating_sub(map_min.x.saturating_sub(2))
            .saturating_add(
                parcel
                    .y
                    .saturating_sub(map_min.y.saturating_sub(2))
                    .saturating_mul(width),
            );
        let parcel_index = parcel_index.clamp(0, (1 << 17) - 1);

        let relative_position =
            position * Vec3::new(1.0, 1.0, -1.0) - (parcel * 16).as_vec2().extend(0.0).xzy();

        let vel_max = velocity.abs().max_element();
        if vel_max <= 4.0 {
            Self::Slow(
                MovementSlow::new()
                    .with_parcel_index(parcel_index as u32)
                    .with_position_x(quantize::<B10>(relative_position.x, 0.0, 16.0))
                    .with_position_y(quantize::<B13>(relative_position.y, 0.0, 200.0))
                    .with_position_z(quantize::<B10>(relative_position.z, 0.0, 16.0))
                    .with_velocity_x(quantize::<B3>(velocity.x.abs(), 0.0, 4.0))
                    .with_velocity_y(quantize::<B3>(velocity.y.abs(), 0.0, 4.0))
                    .with_velocity_z(quantize::<B3>(velocity.z.abs(), 0.0, 4.0))
                    .with_velocity_x_sign(velocity.x < 0.0)
                    .with_velocity_y_sign(velocity.y < 0.0)
                    .with_velocity_z_sign(velocity.z >= 0.0),
            )
        } else {
            let speed = if vel_max < 12.0 { 12.0 } else { 50.0 };
            let inner = MovementFast::new()
                .with_parcel_index(parcel_index as u32)
                .with_position_x(quantize::<B8>(relative_position.x, 0.0, 16.0))
                .with_position_y(quantize::<B13>(relative_position.y, 0.0, 200.0))
                .with_position_z(quantize::<B8>(relative_position.z, 0.0, 16.0))
                .with_velocity_x(quantize::<B5>(velocity.x.abs(), 0.0, speed))
                .with_velocity_y(quantize::<B5>(velocity.y.abs(), 0.0, speed))
                .with_velocity_z(quantize::<B5>(velocity.z.abs(), 0.0, speed))
                .with_velocity_x_sign(velocity.x < 0.0)
                .with_velocity_y_sign(velocity.y < 0.0)
                .with_velocity_z_sign(velocity.z >= 0.0);
            if vel_max < 12.0 {
                Self::Med(inner)
            } else {
                Self::Fast(inner)
            }
        }
    }

    pub fn velocity_tier(&self) -> VelocityTier {
        match self {
            Movement::Slow(_) => VelocityTier::Slow,
            Movement::Med(_) => VelocityTier::Med,
            Movement::Fast(_) => VelocityTier::Fast,
        }
    }

    pub fn parcel_index(&self) -> u32 {
        match self {
            Movement::Slow(val) => val.parcel_index(),
            Movement::Med(val) | Movement::Fast(val) => val.parcel_index(),
        }
    }

    pub fn parcel(&self, map_min: IVec2, map_max: IVec2) -> IVec2 {
        let index = self.parcel_index() as i32;
        let width = map_max
            .x
            .saturating_sub(map_min.x)
            .saturating_add(2 * 2 + 1);
        IVec2::new(
            (index % width).saturating_add(map_min.x.saturating_sub(2)),
            (index / width).saturating_add(map_min.y.saturating_sub(2)),
        )
    }

    pub fn position(&self) -> Vec3 {
        match self {
            Movement::Slow(val) => Vec3::new(
                dequantize::<B10>(val.position_x(), 0.0, 16.0),
                dequantize::<B13>(val.position_y(), 0.0, 200.0),
                dequantize::<B10>(val.position_z(), 0.0, 16.0),
            ),
            Movement::Med(val) | Movement::Fast(val) => Vec3::new(
                dequantize::<B8>(val.position_x(), 0.0, 16.0),
                dequantize::<B13>(val.position_y(), 0.0, 200.0),
                dequantize::<B8>(val.position_z(), 0.0, 16.0),
            ),
        }
    }

    pub fn velocity(&self) -> Vec3 {
        match self {
            Movement::Slow(val) => {
                const SPEED: f32 = 4.0;
                Vec3::new(
                    dequantize::<B3>(val.velocity_x(), 0.0, SPEED)
                        * if val.velocity_x_sign() { -1.0 } else { 1.0 },
                    dequantize::<B3>(val.velocity_y(), 0.0, SPEED)
                        * if val.velocity_y_sign() { -1.0 } else { 1.0 },
                    dequantize::<B3>(val.velocity_z(), 0.0, SPEED)
                        * if val.velocity_z_sign() { -1.0 } else { 1.0 },
                )
            }
            Movement::Med(val) => {
                const SPEED: f32 = 12.0;
                Vec3::new(
                    dequantize::<B5>(val.velocity_x(), 0.0, SPEED)
                        * if val.velocity_x_sign() { -1.0 } else { 1.0 },
                    dequantize::<B5>(val.velocity_y(), 0.0, SPEED)
                        * if val.velocity_y_sign() { -1.0 } else { 1.0 },
                    dequantize::<B5>(val.velocity_z(), 0.0, SPEED)
                        * if val.velocity_z_sign() { -1.0 } else { 1.0 },
                )
            }
            Movement::Fast(val) => {
                const SPEED: f32 = 50.0;
                Vec3::new(
                    dequantize::<B5>(val.velocity_x(), 0.0, SPEED)
                        * if val.velocity_x_sign() { -1.0 } else { 1.0 },
                    dequantize::<B5>(val.velocity_y(), 0.0, SPEED)
                        * if val.velocity_y_sign() { -1.0 } else { 1.0 },
                    dequantize::<B5>(val.velocity_z(), 0.0, SPEED)
                        * if val.velocity_z_sign() { -1.0 } else { 1.0 },
                )
            }
        }
    }

    pub fn into_bytes(self) -> [u8; 8] {
        match self {
            Movement::Slow(val) => val.into_bytes(),
            Movement::Med(val) | Movement::Fast(val) => val.into_bytes(),
        }
    }
}

pub struct MovementCompressed {
    pub temporal: Temporal,
    pub movement: Movement,
}

impl MovementCompressed {
    pub fn from_proto(
        movement: dcl_component::proto_components::kernel::comms::rfc4::MovementCompressed,
    ) -> Self {
        debug!("movement: {movement:?}");
        let temporal = Temporal::from_bytes(movement.temporal_data.to_le_bytes());
        let movement = match temporal.velocity_tier_or_err() {
            Ok(VelocityTier::None) | Ok(VelocityTier::Slow) | Err(_) => Movement::Slow(
                MovementSlow::from_bytes(movement.movement_data.to_le_bytes()),
            ),
            Ok(VelocityTier::Med) => Movement::Med(MovementFast::from_bytes(
                movement.movement_data.to_le_bytes(),
            )),
            Ok(VelocityTier::Fast) => Movement::Fast(MovementFast::from_bytes(
                movement.movement_data.to_le_bytes(),
            )),
        };
        let v = Self { temporal, movement };

        debug!("timestamp: {}", v.temporal.timestamp_f32());
        debug!("kind: {:?}", v.temporal.movement_kind_or_err());
        debug!("rotation: {}", v.temporal.rotation_f32());
        debug!("velocity: {:?}", v.temporal.velocity_tier_or_err());
        debug!("parcel index: {}", v.movement.parcel_index());
        debug!("rel pos: {}", v.movement.position());
        v
    }
    pub fn position(&self, map_min: IVec2, map_max: IVec2) -> Vec3 {
        if map_min == IVec2::MAX {
            return Vec3::ZERO;
        }
        let parcel = self.movement.parcel(map_min, map_max);
        debug!("actual parcel (in {map_min} .. {map_max}: {parcel}");
        ((parcel * 16).as_vec2().extend(0.0).xzy() + self.movement.position())
            * Vec3::new(1.0, 1.0, -1.0)
    }

    pub fn velocity(&self) -> Vec3 {
        self.movement.velocity() * Vec3::new(1.0, 1.0, -1.0)
    }
}
