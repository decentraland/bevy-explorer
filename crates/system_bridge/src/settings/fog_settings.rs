use bevy::prelude::*;
use common::structs::{AppConfig, FogSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for FogSetting {
    fn variants() -> Vec<Self> {
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

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }
}
