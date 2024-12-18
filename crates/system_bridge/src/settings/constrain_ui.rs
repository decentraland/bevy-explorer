use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum ConstrainUiSetting {
    Off,
    On,
}

impl EnumAppSetting for ConstrainUiSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::On]
    }

    fn name(&self) -> String {
        match self {
            ConstrainUiSetting::Off => "Don't Constrain",
            ConstrainUiSetting::On => "Constrain",
        }
        .to_owned()
    }
}

impl AppSetting for ConstrainUiSetting {
    type Param = ();

    fn title() -> String {
        "Constrain Scene Uis".to_owned()
    }

    fn description(&self) -> String {
        format!("Whether to constrain scene uis to render inside the usable area.\n\nScenes can use CanvasInfo to determine what region of the screen should be used for interactive elements, but many scenes don't respect the area correctly.\n\n{}", 
            match self {
                ConstrainUiSetting::Off => "Don't Constrain: Report the full screen space to the scene and trust it to put interactive elements within the usable area.",
                ConstrainUiSetting::On => "Constrain: Lie to the scene about the size of the screen, so that all UI elements are within the usable area."
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.constrain_scene_ui = match self {
            ConstrainUiSetting::Off => false,
            ConstrainUiSetting::On => true,
        };
    }

    fn load(config: &AppConfig) -> Self {
        if config.constrain_scene_ui {
            Self::On
        } else {
            Self::Off
        }
    }

    fn apply(&self, _: (), _: Commands) {}

    fn category() -> super::SettingCategory {
        super::SettingCategory::Gameplay
    }
}
