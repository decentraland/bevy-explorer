use bevy::ecs::system::lifetimeless::{SQuery, Write};
use bevy::math::FloatOrd;
use bevy::prelude::*;
use common::structs::{AppConfig, PrimaryUser};

use super::{AppSetting, IntAppSetting};

macro_rules! player_setting {
    ($struct:ident, $name:expr, $description:expr, $set:expr, $get:expr, $min:expr, $max:expr, $scale: expr) => {
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $struct(FloatOrd);

        impl IntAppSetting for $struct {
            fn from_int(value: i32) -> Self {
                Self(FloatOrd(value as f32 * $scale))
            }

            fn value(&self) -> i32 {
                (self.0.0 / $scale) as i32
            }

            fn min() -> i32 {
                ($min / $scale) as i32
            }

            fn max() -> i32 {
                ($max / $scale) as i32
            }

            fn scale() -> f32 {
                $scale
            }

            fn display(&self) -> String {
                format!("{:.2}", self.0.0)
            }
        }

        #[allow(clippy::redundant_closure_call)]
        impl AppSetting for $struct {
            type Param = SQuery<Write<PrimaryUser>>;

            fn title() -> String {
                format!("{}", $name)
            }

            fn description(&self) -> String {
                format!("{}\n\n{}\n\nFor all player settings, the plan is to make them available to scene authors, to specify for the whole scene or for a trigger area.", $name, $description)
            }

            fn apply(&self, mut q: Query<&mut PrimaryUser>, _: Commands) {
                let Ok(mut settings) = q.single_mut() else {
                    warn!("no primary user");
                    return;
                };
                $set(&mut *settings, self.0.0)
            }

            fn save(&self, config: &mut AppConfig) {
                $set(&mut config.player_settings, self.0.0)
            }

            fn load(config: &AppConfig) -> Self {
                Self(FloatOrd($get(&config.player_settings)))
            }

            fn category() -> super::SettingCategory {
                super::SettingCategory::Gameplay
            }
        }
    };
}

player_setting!(
    RunSpeedSetting,
    "Run Speed",
    "Maximum running speed in m/s. The time to reach this speed will depend on Ground Friction.\nDefault 10",
    |cfg: &mut PrimaryUser, val: f32| cfg.run_speed = val,
    |cfg: &PrimaryUser| cfg.run_speed,
    1.0,
    20.0,
    0.1
);

player_setting!(
    WalkSpeedSetting,
    "Walk Speed",
    "Walking speed in m/s.",
    |cfg: &mut PrimaryUser, val: f32| cfg.walk_speed = val,
    |cfg: &PrimaryUser| cfg.walk_speed,
    1.0,
    20.0,
    0.1
);

player_setting!(
    FrictionSetting,
    "Ground Friction",
    "Traction of the ground. A higher value results in faster starting/stopping, and a lower value simulates a more slippery surface.\nDefault 6",
    |cfg: &mut PrimaryUser, val: f32| cfg.friction = val,
    |cfg: &PrimaryUser| cfg.friction,
    0.1,
    30.0,
    0.1
);

player_setting!(
    GravitySetting,
    "Gravity",
    "Falling force in m/s/s. Higher settings will result in quicker acceleration to terminal velocity, and also make jumps faster (but not higher).\nDefault 20 (9.8 feels very floaty)",
    |cfg: &mut PrimaryUser, val: f32| cfg.gravity = val,
    |cfg: &PrimaryUser| cfg.gravity,
    -1.0,
    -100.0,
    -0.1
);

player_setting!(
    JumpSetting,
    "Jump Height",
    "Maximum height the player can jump, in m. Players can jump onto platforms approximately 0.35m higher than this (due to player's step height).\nDefault 1.25",
    |cfg: &mut PrimaryUser, val: f32| cfg.jump_height = val,
    |cfg: &PrimaryUser| cfg.jump_height,
    1.0,
    20.0,
    0.05
);

player_setting!(
    FallSpeedSetting,
    "Max Fall Speed",
    "Terminal velocity when falling, in m/s. Higher settings will result in eventually falling faster (if you start high enough).\nDefault 15m/s",
    |cfg: &mut PrimaryUser, val: f32| cfg.fall_speed = val,
    |cfg: &PrimaryUser| cfg.fall_speed,
    -0.1,
    -100.0,
    -0.1
);
