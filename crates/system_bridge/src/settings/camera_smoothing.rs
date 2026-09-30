use common::structs::{AppConfig, CameraSmoothing};

use super::{AppSetting, EnumAppSetting, SettingCategory};

impl EnumAppSetting for CameraSmoothing {
    fn variants() -> Vec<Self> {
        vec![Self::Raw, Self::Smoothed, Self::Drunk]
    }

    fn name(&self) -> String {
        match self {
            Self::Raw => "Raw",
            Self::Smoothed => "Smoothed",
            Self::Drunk => "Drunk",
        }
        .to_owned()
    }
}

impl AppSetting for CameraSmoothing {
    type Param = ();

    fn title() -> String {
        "Camera smoothing".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Controls
    }

    fn description(&self) -> String {
        format!(
            "How much the camera rotation is smoothed as you look around.\n\n{}",
            match self {
                Self::Raw => "Raw\nNo smoothing. The camera follows your input directly.",
                Self::Smoothed =>
                    "Smoothed\nA light exponential smoothing takes the edge off sharp movements.",
                Self::Drunk => "Drunk\nHeavy smoothing for a loose, floaty feel.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.camera_smoothing = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.camera_smoothing
    }
}
