use common::structs::AppConfig;

use super::{AppSetting, IntAppSetting, SettingCategory};

#[derive(Debug, PartialEq, Eq)]
pub struct RenderScaleSetting(i32);

impl IntAppSetting for RenderScaleSetting {
    fn from_int(value: i32) -> Self {
        Self(value)
    }

    fn value(&self) -> i32 {
        self.0
    }

    // Players stop at 50%; hosts may go lower for the session (SessionGraphics::MIN_RENDER_SCALE).
    fn min() -> i32 {
        50
    }

    fn max() -> i32 {
        100
    }

    fn display(&self) -> String {
        format!("{}%", self.0)
    }
}

impl AppSetting for RenderScaleSetting {
    type Param = ();

    fn title() -> String {
        "Render Resolution".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Performance
    }

    fn description(&self) -> String {
        "Render Resolution\n\nShare of the screen's pixels the world is drawn at, stretched to fill the window. Lower values reduce GPU load, most on high-density screens, and make the 3D view and in-world UI softer.".to_owned()
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.render_scale = self.0 as f32 / 100.0;
    }

    fn load(config: &AppConfig) -> Self {
        Self(
            ((config.graphics.render_scale * 100.0).round() as i32).clamp(Self::min(), Self::max()),
        )
    }
}
