use bevy::{
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    pbr::ScreenSpaceAmbientOcclusion,
    prelude::*,
};
use common::structs::{AppConfig, PrimaryCameraRes, SsaoSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for SsaoSetting {
    fn variants() -> Vec<Self> {
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
    type Param = SRes<PrimaryCameraRes>;

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

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }

    fn apply(&self, cam_res: SystemParamItem<Self::Param>, commands: Commands) {
        let primary_cam = cam_res.0;
        self.apply_to_camera(&cam_res, commands, primary_cam);
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
            SsaoSetting::Off => cmds.remove::<ScreenSpaceAmbientOcclusion>(),
            SsaoSetting::Low => cmds.insert(ScreenSpaceAmbientOcclusion {
                quality_level: bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                constant_object_thickness: 0.25,
            }),
            SsaoSetting::High => cmds.insert(ScreenSpaceAmbientOcclusion {
                quality_level: bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
                constant_object_thickness: 0.25,
            }),
        };
    }
}
