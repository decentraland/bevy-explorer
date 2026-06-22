use bevy::prelude::*;
use common::structs::{AppConfig, PointAtMarkerVisivbility, PointAtMarkerVisivbilityChanged};

use super::{AppSetting, EnumAppSetting, SettingCategory};

impl EnumAppSetting for PointAtMarkerVisivbility {
    fn variants() -> Vec<Self> {
        vec![Self::All, Self::Friends, Self::None]
    }

    fn name(&self) -> String {
        match self {
            Self::All => "All",
            Self::Friends => "Friends",
            Self::None => "None",
        }
        .to_owned()
    }
}

impl AppSetting for PointAtMarkerVisivbility {
    type Param = ();

    fn title() -> String {
        "Point at marker visibility".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Gameplay
    }

    fn description(&self) -> String {
        format!(
            "Controls which point at markers are shown.\n\n{}",
            match self {
                Self::All => "Show all markers",
                Self::Friends => "Show friends markers",
                Self::None => "Don't show any markers",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.point_at_marker_visibility = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.point_at_marker_visibility
    }

    fn apply(
        &self,
        _param: bevy::ecs::system::SystemParamItem<Self::Param>,
        mut commands: Commands,
        _cameras: &bevy::platform::collections::HashSet<Entity>,
    ) {
        commands.send_event(PointAtMarkerVisivbilityChanged);
    }
}
