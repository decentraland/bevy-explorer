// common phsyical variables (currently only fall speed is used by both foreign avater dynamics and user dynamics...)
use std::f32::consts::PI;

pub const MAX_FALL_SPEED: f32 = 15.0;
pub const GRAVITY: f32 = 20.0;
pub const MAX_CLIMBABLE_INCLINE: f32 = 1.5 * PI / 4.0; // radians from up - equal to 60 degree incline
pub const MAX_STEP_HEIGHT: f32 = 0.5;
pub const MAX_JUMP_HEIGHT: f32 = 1.25;
pub const PLAYER_GROUND_THRESHOLD: f32 = 0.05;

pub const PLAYER_COLLIDER_RADIUS: f32 = 0.35;
pub const PLAYER_COLLIDER_HEIGHT: f32 = 2.0;
pub const PLAYER_COLLIDER_OVERLAP: f32 = 0.01;
