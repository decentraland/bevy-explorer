use std::ops::Deref;

use super::SettingCategory;
use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use common::structs::{AppConfig, ParcelGrassConfig};

use super::{AppSetting, EnumAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub enum ParcelGrassSetting {
    Low,
    Mid,
    High,
}

impl Deref for ParcelGrassSetting {
    type Target = ParcelGrassConfig;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Low => &ParcelGrassConfig {
                layers: 8,
                subdivisions: 32,
                y_displacement: 0.04,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
            Self::Mid => &ParcelGrassConfig {
                layers: 16,
                subdivisions: 32,
                y_displacement: 0.02,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
            Self::High => &ParcelGrassConfig {
                layers: 32,
                subdivisions: 32,
                y_displacement: 0.01,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
        }
    }
}

impl EnumAppSetting for ParcelGrassSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Low, Self::Mid, Self::High]
    }

    fn name(&self) -> String {
        match self {
            ParcelGrassSetting::Low => "Low",
            ParcelGrassSetting::Mid => "Mid",
            ParcelGrassSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for ParcelGrassSetting {
    type Param = SResMut<ParcelGrassConfig>;

    fn title() -> String {
        "Empty Parcel Props".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Graphics
    }

    fn description(&self) -> String {
        format!(
            "Distant scenes are rendered as low quality imposters to increase immersion.\n\n{}",
            match self {
                ParcelGrassSetting::Low => "Low: Sparse grass.",
                ParcelGrassSetting::Mid => "Mid: Average density grass.",
                ParcelGrassSetting::High => "High: Dense grass.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.parcel_grass_config = **self;
    }

    fn load(config: &AppConfig) -> Self {
        match config.parcel_grass_config.layers {
            0..12 => Self::Low,
            12..24 => Self::Mid,
            24.. => Self::High,
        }
    }

    fn apply(
        &self,
        mut param: bevy::ecs::system::SystemParamItem<Self::Param>,
        _commands: Commands,
        _cameras: &bevy::platform::collections::HashSet<Entity>,
    ) {
        *param = **self;
    }
}
