use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, IntAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct SceneThreadsSetting(i32);

impl IntAppSetting for SceneThreadsSetting {
    fn from_int(value: i32) -> Self {
        Self(value)
    }

    fn value(&self) -> i32 {
        self.0
    }

    fn min() -> i32 {
        1
    }

    fn max() -> i32 {
        32
    }
}

impl AppSetting for SceneThreadsSetting {
    type Param = ();

    fn title() -> String {
        "Scene Threads".to_owned()
    }

    fn description(&self) -> String {
        "Scene Threads\n\nNumber of threads to use for running scenes concurrently. A low number will result in infrequent updates to distant scenes. A high number will result in smoother distant scene update frequency, but will increase CPU usage and may impact overall framerate if it is set higher than half the core count of the CPU".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.scene_threads = self.0 as usize;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.scene_threads as i32)
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }

    fn apply(&self, (): (), _: Commands) {
        // handled in scene_runner
    }
}
