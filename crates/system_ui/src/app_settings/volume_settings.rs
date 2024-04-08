use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, AudioSettings};

use super::{spawn_int_setting_template, AppSetting, IntAppSetting};

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

            fn spawn_template(
                commands: &mut Commands,
                dui: &DuiRegistry,
                config: &AppConfig,
            ) -> Entity {
                spawn_int_setting_template::<Self>(commands, dui, config)
            }

            fn apply(&self, mut settings: ResMut<AudioSettings>, _: Commands) {
                $set(&mut *settings, self.0)
            }

            fn save(&self, config: &mut AppConfig) {
                $set(&mut config.audio, self.0)
            }

            fn load(config: &AppConfig) -> Self {
                Self($get(&config.audio))
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

// impl AppSetting for MasterVolumeSetting {
// }
