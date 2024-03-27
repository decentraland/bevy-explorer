use bevy::prelude::*;
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, FogSetting};

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for FogSetting {
    type VParam = ();
    fn variants(_: ()) -> Vec<Self> {
        vec![Self::Off, Self::Basic, Self::Atmospheric]
    }

    fn name(&self) -> String {
        match self {
            FogSetting::Off => "Off",
            FogSetting::Basic => "Basic",
            FogSetting::Atmospheric => "Atmospheric",
        }
        .to_owned()
    }
}

impl AppSetting for FogSetting {
    type Param = ();

    fn title() -> String {
        "Fog".to_owned()
    }

    fn description(&self) -> String {
        format!("Rendering of distant objects. No performance impact, just an aesthetic preference.\n\n{}", 
        match self {
            FogSetting::Off => "Off: No fog adjustment.",
            FogSetting::Basic => "Basic: Distant objects fade into the fog.",
            FogSetting::Atmospheric => "Atmospheric: Distant objects fade into the fog, and distant objects looking towards the sun take the sun color.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.fog = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.fog
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, _: (), _: Commands) {
        // apply is handled by [`visuals::daylight_cycle``]
    }
}
