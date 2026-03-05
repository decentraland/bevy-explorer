use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use common::structs::{AppConfig, AudioSettings};

use super::{AppSetting, IntAppSetting};

macro_rules! volume_setting {
    ($struct:ident, $name:expr, $description:expr, $set:expr, $get:expr) => {
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $struct(i32);

        impl IntAppSetting for $struct {
            fn from_int(value: i32) -> Self {
                Self(value)
            }

            fn value(&self) -> i32 {
                self.0
            }

            fn min() -> i32 {
                0
            }

            fn max() -> i32 {
                100
            }
        }

        #[allow(clippy::redundant_closure_call)]
        impl AppSetting for $struct {
            type Param = SResMut<AudioSettings>;

            fn title() -> String {
                format!("{} Volume", $name)
            }

            fn description(&self) -> String {
                format!("{} Volume\n\n{}", $name, $description)
            }

            fn apply(
                &self,
                mut settings: ResMut<AudioSettings>,
                _: Commands,
                _: &bevy::platform::collections::HashSet<Entity>,
            ) {
                $set(&mut *settings, self.0)
            }

            fn save(&self, config: &mut AppConfig) {
                $set(&mut config.audio, self.0)
            }

            fn load(config: &AppConfig) -> Self {
                Self($get(&config.audio))
            }

            fn category() -> super::SettingCategory {
                super::SettingCategory::Audio
            }
        }
    };
}

volume_setting!(
    MasterVolumeSetting,
    "Master",
    "The overall volume of the application.",
    |cfg: &mut AudioSettings, val: i32| cfg.master = val,
    |cfg: &AudioSettings| cfg.master
);
volume_setting!(
    SceneVolumeSetting,
    "Scene",
    "The volume of music and sound effects played by scenes in the world.",
    |cfg: &mut AudioSettings, val: i32| cfg.scene = val,
    |cfg: &AudioSettings| cfg.scene
);
volume_setting!(
    VoiceVolumeSetting,
    "Voice",
    "The volume of incoming voice audio from other players.",
    |cfg: &mut AudioSettings, val: i32| cfg.voice = val,
    |cfg: &AudioSettings| cfg.voice
);
volume_setting!(
    SystemVolumeSetting,
    "System",
    "The volume of system interactions (menu buttons, etc).",
    |cfg: &mut AudioSettings, val: i32| cfg.system = val,
    |cfg: &AudioSettings| cfg.system
);
volume_setting!(
    AvatarVolumeSetting,
    "Avatar",
    "The volume of avatar emotes and footsteps.",
    |cfg: &mut AudioSettings, val: i32| cfg.avatar = val,
    |cfg: &AudioSettings| cfg.avatar
);

// impl AppSetting for MasterVolumeSetting {
// }
