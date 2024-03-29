use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use bevy_dui::DuiRegistry;
use common::structs::AppConfig;

use super::{spawn_int_setting_template, AppSetting, IntAppSetting};

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

    fn description(&self) -> String {
        "Ambient brightness\n\nHow much brightness is applied to parts of the world not in sunlight.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.ambient_brightness = self.0;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.graphics.ambient_brightness)
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_int_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, mut ambient: ResMut<AmbientLight>, _: Commands) {
        ambient.brightness = (self.0 * 20) as f32;
    }
}
