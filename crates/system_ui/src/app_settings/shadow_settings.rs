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

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for ShadowSetting {
    fn variants() -> Vec<Self> {
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
            ShadowSetting::Low => "Low: Low quality shadows. Uses a single pass shadow map and low quality hardward 2x2 filtering. Gives blocky shadow outlines, particularly with high shadow draw  distances, but is pretty fast.",
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
                *filter_method = ShadowFilteringMethod::Castano13;
            }
        }

        for (mut light, mut cascades) in lights.iter_mut() {
            match self {
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
