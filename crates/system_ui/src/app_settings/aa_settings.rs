use bevy::{
    core_pipeline::fxaa::{Fxaa, Sensitivity},
    ecs::system::{
        lifetimeless::{SRes, SResMut},
        SystemParamItem,
    },
    prelude::*,
};
use bevy_dui::DuiRegistry;
use common::structs::{AaSetting, AppConfig, PrimaryCameraRes};

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for AaSetting {
    fn variants() -> Vec<Self> {
        vec![
            Self::Off,
            Self::FxaaLow,
            Self::FxaaHigh,
            Self::Msaa2x,
            Self::Msaa4x,
            Self::Msaa8x,
        ]
    }

    fn name(&self) -> String {
        match self {
            AaSetting::Off => "Off",
            AaSetting::FxaaLow => "FXAA (Low)",
            AaSetting::FxaaHigh => "FXAA (High)",
            AaSetting::Msaa2x => "MSAA 2x",
            AaSetting::Msaa4x => "MSAA 4x",
            AaSetting::Msaa8x => "MSAA 8x",
        }
        .to_owned()
    }
}

impl AppSetting for AaSetting {
    type Param = (SResMut<Msaa>, SRes<PrimaryCameraRes>);

    fn title() -> String {
        "Anti-aliasing".to_owned()
    }

    fn description(&self) -> String {
        format!("Aliasing reduction strategy. Alternatives for reducing jagged edges in the world.\n\n{}", 
            match self {
                AaSetting::Off => "Off\nNo aliasing is applied. Fastest, lowest quality.",
                AaSetting::FxaaLow => "Fast Approximate Anti-Aliasing\nA blur filter is applied over the entire image rather than just around the edges to give them a smoother look. This method has a lower GPU impact than MSAA, but is less effective and can result in a blurry image.",
                AaSetting::FxaaHigh => "Fast Approximate Anti-Aliasing\nA blur filter is applied over the entire image rather than just around the edges to give them a smoother look. This method has a lower GPU impact than MSAA, but is less effective and can result in a blurry image.",
                AaSetting::Msaa2x => "MSAA 2x\n2x Multisampling of pixels with mesh overlaps. 2x sampling gives a small quality boost for a small GPU cost.",
                AaSetting::Msaa4x => "MSAA 4x\n4x Multisampling of pixels with mesh overlaps. 4x sampling gives a good quality boost for a medium GPU cost.",
                AaSetting::Msaa8x => "MSAA 8x\n8x Multisampling of pixels with mesh overlaps. 8x sampling gives a high quality boost for a high GPU cost.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.msaa = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.msaa
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, (mut msaa, cam_res): SystemParamItem<Self::Param>, mut commands: Commands) {
        *msaa = match self {
            AaSetting::Off | AaSetting::FxaaLow | AaSetting::FxaaHigh => Msaa::Off,
            AaSetting::Msaa2x => Msaa::Sample2,
            AaSetting::Msaa4x => Msaa::Sample4,
            AaSetting::Msaa8x => Msaa::Sample8,
        };

        commands.entity(cam_res.0).remove::<Fxaa>();
        if let Some(sensitivity) = match self {
            AaSetting::FxaaLow => Some(Sensitivity::Medium),
            AaSetting::FxaaHigh => Some(Sensitivity::Ultra),
            _ => None,
        } {
            commands.entity(cam_res.0).insert(Fxaa {
                enabled: true,
                edge_threshold: sensitivity,
                edge_threshold_min: sensitivity,
            });
        }
    }
}
