use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum OobSetting {
    Off,
    On,
}

impl EnumAppSetting for OobSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::On]
    }

    fn name(&self) -> String {
        match self {
            OobSetting::Off => "Off",
            OobSetting::On => "On",
        }
        .to_owned()
    }
}

impl AppSetting for OobSetting {
    type Param = ();

    fn title() -> String {
        "Out-of-bounds Effect".to_owned()
    }

    fn description(&self) -> String {
        format!("How to display out-of-bounds entities.\n\nNOTE: a reload of live scenes is required for changes to this setting to take effect.\n\n{}", 
            match self {
                OobSetting::Off => "Off: Out-of-bounds fragments are discarded. Fastest performance.",
                OobSetting::On => "On: Out-of-bounds fragments are dissolved with a simplex noise calculation. Slower performance."
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.oob = match self {
            OobSetting::Off => 0.0,
            OobSetting::On => 2.0,
        };
    }

    fn load(config: &AppConfig) -> Self {
        if config.graphics.oob == 0.0 {
            Self::Off
        } else {
            Self::On
        }
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }

    fn apply(&self, _: (), _: Commands) {
        // setting is handled in the places where materials are created
    }
}
