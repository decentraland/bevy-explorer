use super::SettingCategory;
use bevy::{
    core_pipeline::dof::{DepthOfField, DepthOfFieldMode},
    ecs::system::SystemParamItem,
    prelude::*,
};
use common::structs::{AppConfig, DofConfig, DofSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for DofSetting {
    fn variants() -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            DofSetting::Off => "Off",
            DofSetting::Low => "Low",
            DofSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for DofSetting {
    type Param = ();

    fn title() -> String {
        "Depth of Field".to_owned()
    }

    fn category() -> SettingCategory {
        SettingCategory::Graphics
    }

    fn description(&self) -> String {
        format!("Bloom is a post-processing effect used to reproduce an imaging artifact of real-world cameras. The effect produces fringes (or feathers) of light extending from the borders of bright areas in an image, contributing to the illusion of an extremely bright light overwhelming the camera capturing the scene.\n\n{}", 
        match self {
            DofSetting::Off => "Off: No depth of field.",
            DofSetting::Low => "Low: A bit of depth of field is applied.",
            DofSetting::High => "High: Lots of depth of field is applied.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.dof = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.dof
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

        let (mode, aperture_f_stops) = match self {
            // no blur, but depth of field stays active so its transparent focus texture is still
            // rendered: the stars read it to hide behind imposters
            DofSetting::Off => (DepthOfFieldMode::Disabled, 0.15),
            DofSetting::Low => (DepthOfFieldMode::Gaussian, 0.15),
            DofSetting::High => (DepthOfFieldMode::Gaussian, 0.05),
        };
        cmds.try_insert((
            DepthOfField {
                mode,
                focal_distance: 50.0, // updated based on cam + extra
                aperture_f_stops,
                max_depth: 250.0,
                max_circle_of_confusion_diameter: 20.0,
                sensor_height: 0.06,
            },
            DofConfig {
                default_sensor_height: 0.06,
                extra_focal_distance: 50.0,
            },
        ));
    }
}
