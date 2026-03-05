use bevy::prelude::*;
use common::structs::AppConfig;

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum CacheSizeSetting {
    Disabled,
    Gb1,
    Gb10,
    Gb100,
    Gb1000,
    Unlimited,
}

impl EnumAppSetting for CacheSizeSetting {
    fn variants() -> Vec<Self> {
        vec![
            Self::Disabled,
            Self::Gb1,
            Self::Gb10,
            Self::Gb100,
            Self::Gb1000,
            Self::Unlimited,
        ]
    }

    fn name(&self) -> String {
        match self {
            CacheSizeSetting::Disabled => "Disabled",
            CacheSizeSetting::Gb1 => "1 Gb",
            CacheSizeSetting::Gb10 => "10 Gb",
            CacheSizeSetting::Gb100 => "100 Gb",
            CacheSizeSetting::Gb1000 => "1000 Gb",
            CacheSizeSetting::Unlimited => "Unlimited",
        }
        .to_owned()
    }
}

impl AppSetting for CacheSizeSetting {
    type Param = ();

    fn title() -> String {
        "Disk Cache Size".to_owned()
    }

    fn description(&self) -> String {
        format!("Size of the disk cache for scene data.\n\n The cache is only cleared on exit, so may exceed this size while the application is running.\n\n{}", 
            match self {
                CacheSizeSetting::Disabled => "The cache will be fully cleared on exit.",
                CacheSizeSetting::Gb1 => "1 Gb",
                CacheSizeSetting::Gb10 => "10 Gb",
                CacheSizeSetting::Gb100 => "100 Gb",
                CacheSizeSetting::Gb1000 => "1000 Gb",
                CacheSizeSetting::Unlimited => "The cache will never be cleared."
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.cache_bytes = 1024
            * 1024
            * 1024
            * match self {
                CacheSizeSetting::Disabled => 0,
                CacheSizeSetting::Gb1 => 1,
                CacheSizeSetting::Gb10 => 10,
                CacheSizeSetting::Gb100 => 100,
                CacheSizeSetting::Gb1000 => 1000,
                CacheSizeSetting::Unlimited => 1000000000,
            };
    }

    fn load(config: &AppConfig) -> Self {
        let gb = config.cache_bytes / 1024 / 1024 / 1024;
        if gb < 1 {
            Self::Disabled
        } else if gb < 10 {
            Self::Gb1
        } else if gb < 100 {
            Self::Gb10
        } else if gb < 1000 {
            Self::Gb100
        } else if gb < 10000 {
            Self::Gb1000
        } else {
            Self::Unlimited
        }
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }
}
