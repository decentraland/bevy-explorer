use super::SettingCategory;
use bevy::{ecs::system::lifetimeless::SResMut, platform::collections::HashSet, prelude::*};
use common::structs::AppConfig;

use super::{AppSetting, IntAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct AmbientSetting(i32);

impl IntAppSetting for AmbientSetting {
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

impl AppSetting for AmbientSetting {
    type Param = SResMut<AmbientLight>;

    fn title() -> String {
        "Ambient Brightness".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Graphics
    }

    fn description(&self) -> String {
        "Ambient brightness\n\nHow much brightness is applied to parts of the world not in sunlight.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.ambient_brightness = self.0;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.graphics.ambient_brightness)
    }

    fn apply(&self, mut ambient: ResMut<AmbientLight>, _: Commands, _: &HashSet<Entity>) {
        ambient.brightness = (self.0 * 20) as f32;
    }
}
