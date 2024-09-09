use bevy::{
    ecs::system::{
        lifetimeless::{SQuery, SRes, Write},
        SystemParamItem,
    },
    pbr::{CascadeShadowConfig, CascadeShadowConfigBuilder, ShadowFilteringMethod},
    prelude::*,
};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, PrimaryCameraRes, ShadowSetting};

use super::{
    spawn_enum_setting_template, spawn_int_setting_template, AppSetting, EnumAppSetting,
    IntAppSetting,
};

impl EnumAppSetting for ShadowSetting {
    type VParam = ();
    fn variants(_: ()) -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            ShadowSetting::Off => "Off",
            ShadowSetting::Low => "Low",
            ShadowSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for ShadowSetting {
    type Param = (
        SRes<AppConfig>,
        SRes<PrimaryCameraRes>,
        SQuery<Write<ShadowFilteringMethod>>,
        SQuery<(Write<DirectionalLight>, Write<CascadeShadowConfig>)>,
    );

    fn title() -> String {
        "Shadow settings".to_owned()
    }

    fn description(&self) -> String {
        format!("How shadows are rendered in the world.\n\n{}", 
        match self {
            ShadowSetting::Off => "Off: No shadows are rendered. Fastest",
            ShadowSetting::Low => "Low: Low quality shadows. Uses a single pass shadow map and low quality hardward 2x2 filtering. Gives blocky shadow outlines, particularly with high shadow draw distances, but is pretty fast.",
            ShadowSetting::High => "High: Higher quality shadows. Uses a set of cascaded shadow maps and higher quality filtering for softer shadow outlines and better quality at higher shadow draw distances, but is more GPU intensive.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.shadow_settings = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.shadow_settings
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(
        &self,
        (config, cam_res, mut filter_method, mut lights): SystemParamItem<Self::Param>,
        _: Commands,
    ) {
        let mut filter_method = filter_method.get_mut(cam_res.0).unwrap();

        match self {
            ShadowSetting::Off => (),
            ShadowSetting::Low => {
                *filter_method = ShadowFilteringMethod::Hardware2x2;
            }
            ShadowSetting::High => {
                *filter_method = ShadowFilteringMethod::Gaussian;
            }
        }

        let value = if config.graphics.shadow_distance == 0.0 {
            ShadowSetting::Off
        } else {
            *self
        };

        for (mut light, mut cascades) in lights.iter_mut() {
            match value {
                ShadowSetting::Off => {
                    light.shadows_enabled = false;
                }
                ShadowSetting::Low => {
                    light.shadows_enabled = true;
                    *cascades = CascadeShadowConfigBuilder {
                        num_cascades: 1,
                        minimum_distance: 0.1,
                        maximum_distance: config.graphics.shadow_distance,
                        first_cascade_far_bound: config.graphics.shadow_distance,
                        overlap_proportion: 0.2,
                    }
                    .build()
                }
                ShadowSetting::High => {
                    light.shadows_enabled = true;
                    *cascades = CascadeShadowConfigBuilder {
                        num_cascades: 4,
                        minimum_distance: 0.1,
                        maximum_distance: config.graphics.shadow_distance,
                        first_cascade_far_bound: config.graphics.shadow_distance / 15.0,
                        overlap_proportion: 0.2,
                    }
                    .build()
                }
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

    fn apply(&self, _: (), _: Commands) {
        // applied via ShadowSetting
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_int_setting_template::<Self>(commands, dui, config)
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

    fn apply(&self, _: (), _: Commands) {
        // applied via lights system
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_int_setting_template::<Self>(commands, dui, config)
    }
}
