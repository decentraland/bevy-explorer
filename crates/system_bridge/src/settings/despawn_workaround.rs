use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum DespawnWorkaroundSetting {
    Off,
    On,
}

impl EnumAppSetting for DespawnWorkaroundSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::On]
    }

    fn name(&self) -> String {
        match self {
            DespawnWorkaroundSetting::Off => "Off",
            DespawnWorkaroundSetting::On => "On",
        }
        .to_owned()
    }
}

impl AppSetting for DespawnWorkaroundSetting {
    type Param = ();

    fn title() -> String {
        "Despawn Workaround".to_owned()
    }

    fn description(&self) -> String {
        "Despawn Workaround.\n\nOn some linux systems, despawning multiple v8 engines simultaneously causes seg faults. This workaround can be enabled to throttle the scene despawn rate to avoid this crash.".to_owned()
    }

    fn save(&self, config: &mut AppConfig) {
        config.despawn_workaround = matches!(self, DespawnWorkaroundSetting::On);
    }

    fn load(config: &AppConfig) -> Self {
        if config.despawn_workaround {
            Self::On
        } else {
            Self::Off
        }
    }

    fn apply(&self, _: (), _: Commands) {
        // setting is handled in scene_runner::process_scene_lifecycle
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }
}
