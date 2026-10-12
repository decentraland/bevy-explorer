use common::structs::{AppConfig, SkyReflectionSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for SkyReflectionSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            SkyReflectionSetting::Low => "Low",
            SkyReflectionSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for SkyReflectionSetting {
    // the sky env map reads the config
    type Param = ();

    fn title() -> String {
        "Sky Reflections".to_owned()
    }

    fn description(&self) -> String {
        format!(
            "Sky Reflections\n\nThe sky lights the world as its ambient light, and shiny surfaces reflect it.\n{}",
            match self {
                SkyReflectionSetting::Low => "Low: one colour each for the sky, the horizon and the ground, for next to no GPU cost.",
                SkyReflectionSetting::High => "High: reflections of the sky itself, blurred to match the surface, for a small GPU cost every frame.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.sky_reflections = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.sky_reflections
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }
}
