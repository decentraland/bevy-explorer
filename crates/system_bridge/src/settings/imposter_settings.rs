use super::SettingCategory;
use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use common::structs::{AppConfig, SceneLoadDistance};

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum ImposterSetting {
    Off,
    Low,
    High,
}

impl EnumAppSetting for ImposterSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            ImposterSetting::Off => "Off",
            ImposterSetting::Low => "Normal",
            ImposterSetting::High => "Ultra",
        }
        .to_owned()
    }
}

impl AppSetting for ImposterSetting {
    type Param = SResMut<SceneLoadDistance>;

    fn title() -> String {
        "Distant Scene Rendering".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Graphics
    }

    fn description(&self) -> String {
        format!("Distant scenes are rendered as low quality imposters to increase immersion.\n\n{}", 
        match self {
            ImposterSetting::Off => "Off: No distant scenes are rendered.",
            ImposterSetting::Low => "Normal: Distant scenes are rendered at normal quality.",
            ImposterSetting::High => "Ultra: Distant scenes are rendered at higher quality. This setting requires at least 16mb VRAM.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.scene_imposter_distances = match self {
            ImposterSetting::Off => vec![],
            ImposterSetting::Low => vec![100.0, 200.0, 400.0, 800.0, 1600.0, 99999.0],
            ImposterSetting::High => vec![200.0, 400.0, 800.0, 1600.0, 99999.0],
        };
        config.scene_imposter_multisample = false;
        config.scene_imposter_multisample_amount = 0.0;
    }

    fn load(config: &AppConfig) -> Self {
        match config.scene_imposter_distances.first() {
            None => Self::Off,
            Some(200.0) => Self::High,
            _ => Self::Low,
        }
    }

    fn apply(
            &self,
            mut param: bevy::ecs::system::SystemParamItem<Self::Param>,
            _commands: Commands,
            _cameras: &bevy::platform::collections::HashSet<Entity>,
        ) {
        let max_distance = match self {
            ImposterSetting::Off => 0.0,
            _ => 99999.0,
        };
        
        param.load_imposter = max_distance;
    }
}
