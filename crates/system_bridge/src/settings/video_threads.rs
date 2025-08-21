use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, IntAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct VideoThreadsSetting(i32);

impl IntAppSetting for VideoThreadsSetting {
    fn from_int(value: i32) -> Self {
        Self(value)
    }

    fn value(&self) -> i32 {
        self.0
    }

    fn min() -> i32 {
        0
    }

    fn max() -> i32 {
        32
    }
}

impl AppSetting for VideoThreadsSetting {
    type Param = ();

    fn title() -> String {
        "Max Videos".to_owned()
    }

    fn description(&self) -> String {
        "Max AV Sources\n\nMaximum number of audio streams and videos to process simultaneously. Allowing more AV sources puts a higher burden on both CPU and GPU.\nIf scenes spawn more audio and video sources than this maximum, more distant sources from the player will be paused.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.max_videos = self.0 as usize;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.max_videos as i32)
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }

}
