use bevy::prelude::*;
use common::structs::{AppConfig, DmPrivacy};

use super::{AppSetting, EnumAppSetting, SettingCategory};

impl EnumAppSetting for DmPrivacy {
    fn variants() -> Vec<Self> {
        vec![Self::All, Self::OnlyFriends]
    }

    fn name(&self) -> String {
        match self {
            Self::All => "Everyone",
            Self::OnlyFriends => "Only friends",
        }
        .to_owned()
    }

    fn is_unset(config: &AppConfig) -> bool {
        config.dm_privacy.is_none()
    }
}

impl AppSetting for DmPrivacy {
    type Param = ();

    fn title() -> String {
        "Direct messages".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Chat
    }

    fn description(&self) -> String {
        format!(
            "Who can send you direct messages.\n\n{}",
            match self {
                Self::All => "Anyone can message you",
                Self::OnlyFriends => "Only your friends can message you",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.dm_privacy = Some(*self);
    }

    fn load(config: &AppConfig) -> Self {
        config.dm_privacy.unwrap_or_default()
    }

    // The social crate watches the config slot and upserts to the social service.
    fn apply(
        &self,
        _param: bevy::ecs::system::SystemParamItem<Self::Param>,
        _commands: Commands,
        _cameras: &bevy::platform::collections::HashSet<Entity>,
    ) {
    }
}
