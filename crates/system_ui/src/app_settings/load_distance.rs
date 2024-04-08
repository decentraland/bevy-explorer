use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, SceneLoadDistance};

use super::{spawn_int_setting_template, AppSetting, IntAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct LoadDistanceSetting(i32);

impl IntAppSetting for LoadDistanceSetting {
    fn from_int(value: i32) -> Self {
        Self(value)
    }

    fn value(&self) -> i32 {
        self.0
    }

    fn min() -> i32 {
        10
    }

    fn max() -> i32 {
        150
    }
}

impl AppSetting for LoadDistanceSetting {
    type Param = SResMut<SceneLoadDistance>;

    fn title() -> String {
        "Scene Load Distance".to_owned()
    }

    fn description(&self) -> String {
        "Scene Load Distance\n\nThe distance at which neighbouring scenes will be spawned. A scene is 16 meters, so for example a value of 64 will load 4 scenes in all directions".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.scene_load_distance = self.0 as f32;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.scene_load_distance as i32)
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_int_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, mut d: ResMut<SceneLoadDistance>, _: Commands) {
        d.load = self.0 as f32;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnloadDistanceSetting(i32);

impl IntAppSetting for UnloadDistanceSetting {
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

impl AppSetting for UnloadDistanceSetting {
    type Param = SResMut<SceneLoadDistance>;

    fn title() -> String {
        "Scene Unload Distance".to_owned()
    }

    fn description(&self) -> String {
        "Scene Unload Distance\n\nThe additional distance (above the load distance) at which neighbouring scenes will be despawned. Using too low a setting will cause churn as scenes load and unload frequently.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.scene_unload_extra_distance = self.0 as f32;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.scene_unload_extra_distance as i32)
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_int_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, mut d: ResMut<SceneLoadDistance>, _: Commands) {
        d.unload = self.0 as f32;
    }
}
