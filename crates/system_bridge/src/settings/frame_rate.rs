use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct FpsTargetSetting(usize);

impl EnumAppSetting for FpsTargetSetting {
    fn variants() -> Vec<Self> {
        vec![
            Self(10),
            Self(15),
            Self(20),
            Self(30),
            Self(60),
            Self(120),
            Self(144),
            Self(999),
        ]
    }

    fn name(&self) -> String {
        if self.0 == 999 {
            "Uncapped".to_owned()
        } else {
            format!("{} fps", self.0)
        }
    }
}

impl AppSetting for FpsTargetSetting {
    type Param = ();
    fn title() -> String {
        "Target Frame Rate".to_owned()
    }

    fn description(&self) -> String {
        "The target frame rate. Lower values may be uncomfortable or jerky, while higher values will result in a smoother experience but increased CPU and GPU load.\n\n".to_owned()
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.fps_target = self.0;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.graphics.fps_target)
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }
}
