use bevy::prelude::*;
use common::structs::AppConfig;
use ipfs::IpfsAssetServer;

use super::{AppSetting, IntAppSetting};

#[derive(Debug, PartialEq, Eq)]
pub struct MaxDownloadsSetting(i32);

impl IntAppSetting for MaxDownloadsSetting {
    fn from_int(value: i32) -> Self {
        Self(value)
    }

    fn value(&self) -> i32 {
        self.0
    }

    fn min() -> i32 {
        1
    }

    fn max() -> i32 {
        64
    }
}

impl AppSetting for MaxDownloadsSetting {
    type Param = IpfsAssetServer<'static, 'static>;

    fn title() -> String {
        "Max Downloads".to_owned()
    }

    fn description(&self) -> String {
        "Max Downloads\n\nMaximum number of simultaneous downloads to allow. Higher numbers may cause more PCIE/GPU memory pressure and more network usage, potentially leading to hiccups, but may also result in scenes loading faster.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.max_concurrent_remotes = self.0 as usize;
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.max_concurrent_remotes as i32)
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Performance
    }

    fn apply(&self, ipfas: IpfsAssetServer, _: Commands) {
        ipfas.ipfs().set_concurrent_remote_count(self.0 as usize)
    }
}
