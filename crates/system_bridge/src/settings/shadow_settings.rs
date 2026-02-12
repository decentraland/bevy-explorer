use bevy::{
    ecs::system::{
        lifetimeless::{SQuery, SRes, SResMut, Write},
        SystemParamItem,
    },
    pbr::{CascadeShadowConfig, DirectionalLightShadowMap, ShadowFilteringMethod},
    platform::collections::HashSet,
    prelude::*,
};
use common::structs::{AppConfig, PrimaryCameraRes, ShadowSetting};

use super::{AppSetting, EnumAppSetting, IntAppSetting};

impl EnumAppSetting for ShadowSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::Middle, Self::High]
    }

    fn name(&self) -> String {
        match self {
            ShadowSetting::Off => "Off",
            ShadowSetting::Low => "Low",
            ShadowSetting::Middle => "Middle",
            ShadowSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for ShadowSetting {
    type Param = (
        SRes<AppConfig>,
        SRes<PrimaryCameraRes>,
        SQuery<(Write<DirectionalLight>, Write<CascadeShadowConfig>)>,
        SResMut<DirectionalLightShadowMap>,
    );

    fn title() -> String {
        "Shadow settings".to_owned()
    }

    fn description(&self) -> String {
        format!("How shadows are rendered in the world.\n\n{}",
        match self {
            ShadowSetting::Off => "Off: No shadows are rendered. Fastest performance.",
            ShadowSetting::Low => "Low: Basic shadows with 512px resolution. Uses a single cascade shadow map and hardware 2x2 filtering. Fast but blocky shadows at distance.",
            ShadowSetting::Middle => "Middle: Balanced shadows with 1024px resolution. Uses 2 cascade shadow maps and Gaussian filtering. Good quality/performance balance.",
            ShadowSetting::High => "High: Premium shadows with 4096px resolution. Uses 4 cascade shadow maps and Gaussian filtering. Best quality but GPU intensive.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.shadow_settings = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.shadow_settings
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }

    fn apply(
        &self,
        (config, cam_res, mut lights, mut shadow_map): SystemParamItem<Self::Param>,
        mut commands: Commands,
        cameras: &HashSet<Entity>,
    ) {
        let value = if config.graphics.shadow_distance == 0.0 {
            ShadowSetting::Off
        } else {
            *self
        };

        for (mut light, mut cascades) in lights.iter_mut() {
            let (shadows_enabled, cascade_config, shadow_map_size) =
                value.to_shadow_config(config.graphics.shadow_distance);
            light.shadows_enabled = shadows_enabled;
            *cascades = cascade_config;
            // Update shadow map resolution based on quality setting
            shadow_map.size = shadow_map_size;
        }

        let res = &(config, cam_res, lights, shadow_map);
        for &cam in cameras {
            self.apply_to_camera(res, commands.reborrow(), cam);
        }
    }

    fn apply_to_camera(
        &self,
        _: &SystemParamItem<Self::Param>,
        mut commands: Commands,
        camera_entity: Entity,
    ) {
        let Ok(mut cmds) = commands.get_entity(camera_entity) else {
            return;
        };
        match self {
            ShadowSetting::Off => (),
            ShadowSetting::Low => {
                cmds.insert(ShadowFilteringMethod::Hardware2x2);
            }
            ShadowSetting::Middle => {
                cmds.insert(ShadowFilteringMethod::Gaussian); // Balanced performance
            }
            ShadowSetting::High => {
                cmds.insert(ShadowFilteringMethod::Gaussian); // Best quality
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShadowDistanceSetting(i32);

impl IntAppSetting for ShadowDistanceSetting {
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
        300
    }
}

impl AppSetting for ShadowDistanceSetting {
    type Param = ();

    fn title() -> String {
        "Shadow Distance".to_owned()
    }

    fn description(&self) -> String {
        "Shadow Distance\n\nDistance up to which to render shadows. To ensure that shadows are rendered even for large scenes which are at the edge of the loaded area, this can be set to a higher value (e.g. 2x) than the scene load distance. Higher values increase GPU time slightly, and may result in lower quality shadows at closer distances (particularly with Low shadow quality).".to_owned()
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.graphics.shadow_distance as i32)
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.shadow_distance = self.0 as f32
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShadowCasterCountSetting(i32);

impl IntAppSetting for ShadowCasterCountSetting {
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
        64
    }
}

impl AppSetting for ShadowCasterCountSetting {
    type Param = ();

    fn title() -> String {
        "Shadow Caster Count".to_owned()
    }

    fn description(&self) -> String {
        "Shadow Caster Count\n\nMaximum number of scene lights (excluding the global sun light) that will cast shadows. Lights that cast shadows are expensive in GPU time and memory, the cost scales linearly with the number of shadow-casting lights. Reduce this if scenes with a large number of shadow-casting lights cause slowdown.\n\nLights from currently active scenes, closest to the player, will have shadows enabled first.".to_owned()
    }

    fn load(config: &AppConfig) -> Self {
        Self(config.graphics.shadow_caster_count as i32)
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.shadow_caster_count = self.0 as usize
    }

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }
}
