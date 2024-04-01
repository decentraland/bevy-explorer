use bevy::{
    core_pipeline::bloom::BloomSettings,
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    prelude::*,
};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, BloomSetting, PrimaryCameraRes};

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for BloomSetting {
    type VParam = ();
    fn variants(_: ()) -> Vec<Self> {
        vec![Self::Off, Self::Low, Self::High]
    }

    fn name(&self) -> String {
        match self {
            BloomSetting::Off => "Off",
            BloomSetting::Low => "Low",
            BloomSetting::High => "High",
        }
        .to_owned()
    }
}

impl AppSetting for BloomSetting {
    type Param = SRes<PrimaryCameraRes>;

    fn title() -> String {
        "Bloom".to_owned()
    }

    fn description(&self) -> String {
        format!("Bloom is a post-processing effect used to reproduce an imaging artifact of real-world cameras. The effect produces fringes (or feathers) of light extending from the borders of bright areas in an image, contributing to the illusion of an extremely bright light overwhelming the camera capturing the scene.\n\n{}", 
        match self {
            BloomSetting::Off => "Off: No bloom.",
            BloomSetting::Low => "Low: A subtle bloom effect is applied.",
            BloomSetting::High => "High: The bloom effect smacks you in the face and makes it hard to see.",
        })
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.bloom = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.bloom
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, cam_res: SystemParamItem<Self::Param>, mut commands: Commands) {
        let mut cmds = commands.entity(cam_res.0);

        match self {
            BloomSetting::Off => cmds.remove::<BloomSettings>(),
            BloomSetting::Low => cmds.insert(BloomSettings {
                intensity: 0.10,
                ..BloomSettings::NATURAL
            }),
            BloomSetting::High => cmds.insert(BloomSettings {
                intensity: 0.10,
                ..BloomSettings::OLD_SCHOOL
            }),
        };
    }
}
