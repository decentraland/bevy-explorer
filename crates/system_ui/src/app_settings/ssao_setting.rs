use bevy::{
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    pbr::{ScreenSpaceAmbientOcclusionBundle, ScreenSpaceAmbientOcclusionSettings},
    prelude::*,
};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, PrimaryCameraRes, SsaoSetting};

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for SsaoSetting {
    type VParam = ();
    fn variants(_: ()) -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            SsaoSetting::Off => "Off",
            SsaoSetting::Low => "Low",
            SsaoSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for SsaoSetting {
    type Param = (SRes<PrimaryCameraRes>, SRes<Msaa>);

    fn title() -> String {
        "SSAO".to_owned()
    }

    fn description(&self) -> String {
        format!("Screen-space Ambient Occlusion\n\nA subtle effect to apply shadows based on visible screen-space geometry, to darken corners and give a more physical impression of ambient light.\nNOTE: SSAO cannot work concurrently with multisampled MSAA. For this setting to have any effect Anti-Aliasing must be set to OFF or FXAA.\n{}", 
        match self {
            SsaoSetting::Off => "Off: No SSAO.",
            SsaoSetting::Low => "Low: a low quality ambient occlusion for a medium GPU cost.",
            SsaoSetting::High => "High: a higher quality ambient occlusion for a higher GPU cost.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.ssao = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.ssao
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, (cam_res, msaa_res): SystemParamItem<Self::Param>, mut commands: Commands) {
        let mut cmds = commands.entity(cam_res.0);

        if self != &SsaoSetting::Off && msaa_res.samples() > 1 {
            warn!("SSAO disabled due to MSAA setting");
        }

        match (msaa_res.samples() > 1, self) {
            (_, SsaoSetting::Off) | (true, _) => cmds.remove::<ScreenSpaceAmbientOcclusionBundle>(),
            (false, SsaoSetting::Low) => cmds.insert(ScreenSpaceAmbientOcclusionBundle {
                settings: ScreenSpaceAmbientOcclusionSettings {
                    quality_level: bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                },
                ..Default::default()
            }),
            (false, SsaoSetting::High) => cmds.insert(ScreenSpaceAmbientOcclusionBundle {
                settings: ScreenSpaceAmbientOcclusionSettings {
                    quality_level: bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
                },
                ..Default::default()
            }),
        };
    }
}
