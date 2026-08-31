use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum CelShadingSetting {
    Off,
    On,
}

impl EnumAppSetting for CelShadingSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::On]
    }

    fn name(&self) -> String {
        match self {
            CelShadingSetting::Off => "Off",
            CelShadingSetting::On => "On",
        }
        .to_owned()
    }
}

impl AppSetting for CelShadingSetting {
    type Param = ();

    fn title() -> String {
        "Avatar Cel Shading".to_owned()
    }

    fn description(&self) -> String {
        "Cel (toon) shading for avatars.\n\nOn: avatars are flat, banded toon shading, consistent day and night. Off: avatars use standard PBR lighting like the rest of the world.".to_owned()
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.cel_shading = matches!(self, CelShadingSetting::On);
    }

    fn load(config: &AppConfig) -> Self {
        if config.graphics.cel_shading {
            Self::On
        } else {
            Self::Off
        }
    }
}
