use super::SettingCategory;
use bevy::{ecs::system::lifetimeless::SResMut, prelude::*};
use common::structs::{AppConfig, ParcelGrassConfig, ParcelGrassSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for ParcelGrassSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::Mid, Self::High]
    }

    fn name(&self) -> String {
        match self {
            ParcelGrassSetting::Off => "Off",
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
            "Controls the density of the grass on empty parcels.\n\n{}",
            match self {
                ParcelGrassSetting::Off => "Off: No grass.",
                ParcelGrassSetting::Low => "Low: Sparse grass.",
                ParcelGrassSetting::Mid => "Mid: Average density grass.",
                ParcelGrassSetting::High => "High: Dense grass.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.parcel_grass_setting = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.parcel_grass_setting
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
