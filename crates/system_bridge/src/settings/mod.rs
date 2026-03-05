use std::fmt::Display;

use crate::{
    settings::{imposter_settings::ImposterSetting, sensitivity::*},
    SystemApi,
};
use ambient_brightness_setting::AmbientSetting;
use anyhow::anyhow;
use bevy::{
    app::{Plugin, Update},
    ecs::{
        schedule::ScheduleLabel,
        system::{StaticSystemParam, SystemParam, SystemParamItem},
    },
    platform::collections::HashSet,
    prelude::*,
};
use cache_size::CacheSizeSetting;
#[cfg(not(target_arch = "wasm32"))]
use common::structs::SsaoSetting;
use common::{
    sets::SceneSets,
    structs::{
        AaSetting, AppConfig, BloomSetting, DofSetting, FogSetting, PreviewMode, ShadowSetting,
        WindowSetting,
    },
};
use constrain_ui::ConstrainUiSetting;
use frame_rate::FpsTargetSetting;
use load_distance::{LoadDistanceSetting, UnloadDistanceSetting};
use max_avatars::MaxAvatarsSetting;
use max_downloads::MaxDownloadsSetting;
use oob_setting::OobSetting;
use player_settings::{
    FallSpeedSetting, FrictionSetting, GravitySetting, JumpSetting, RunSpeedSetting,
    WalkSpeedSetting,
};
use scene_threads::SceneThreadsSetting;
use serde::{Deserialize, Serialize};
use shadow_settings::{ShadowCasterCountSetting, ShadowDistanceSetting};
use video_threads::VideoThreadsSetting;
use volume_settings::{
    AvatarVolumeSetting, MasterVolumeSetting, SceneVolumeSetting, SystemVolumeSetting,
    VoiceVolumeSetting,
};

pub mod aa_settings;
pub mod ambient_brightness_setting;
pub mod bloom_settings;
pub mod cache_size;
pub mod constrain_ui;
pub mod dof_setting;
pub mod fog_settings;
pub mod frame_rate;
pub mod imposter_settings;
pub mod load_distance;
pub mod max_avatars;
pub mod max_downloads;
pub mod oob_setting;
pub mod player_settings;
pub mod scene_threads;
pub mod sensitivity;
pub mod shadow_settings;
pub mod ssao_setting;
pub mod video_threads;
pub mod volume_settings;
pub mod window_settings;

pub struct SettingBridgePlugin;

#[derive(Event)]
pub struct NewCameraEvent(pub Entity);

impl Plugin for SettingBridgePlugin {
    fn build(&self, app: &mut App) {
        let is_preview = app.world().resource::<PreviewMode>().is_preview;

        fn apply_to_camera<S: AppSetting>(
            mut commands: Commands,
            config: Res<AppConfig>,
            mut new_camera_events: EventReader<NewCameraEvent>,
            param: StaticSystemParam<S::Param>,
        ) {
            let param = param.into_inner();
            for ev in new_camera_events.read() {
                let setting = S::load(&config);
                setting.apply_to_camera(&param, commands.reborrow(), ev.0);
            }
        }

        fn add_int_setting<T: IntAppSetting>(
            app: &mut App,
            settings: &mut Settings,
            schedule: &mut Schedule,
            config: &AppConfig,
        ) {
            settings.add_int_setting::<T>(config);
            schedule.add_systems(apply_setting::<T>);
            app.add_systems(
                Update,
                apply_to_camera::<T>.in_set(SceneSets::RestrictedActions),
            );
        }

        fn add_enum_setting<T: EnumAppSetting>(
            app: &mut App,
            settings: &mut Settings,
            schedule: &mut Schedule,
            config: &AppConfig,
        ) {
            settings.add_enum_setting::<T>(config);
            schedule.add_systems(apply_setting::<T>);
            app.add_systems(
                Update,
                apply_to_camera::<T>.in_set(SceneSets::RestrictedActions),
            );
        }

        let mut settings = Settings {
            settings: Vec::default(),
        };
        app.add_event::<NewCameraEvent>();
        app.add_systems(Update, (send_settings, receive_settings));

        let mut schedule = Schedule::new(ApplyAppSettingsLabel);
        let config = app.world().resource::<AppConfig>().clone();

        add_int_setting::<ShadowDistanceSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<ShadowCasterCountSetting>(app, &mut settings, &mut schedule, &config);

        // special case for ordering
        settings.add_enum_setting::<ShadowSetting>(&config);
        schedule.add_systems(
            apply_setting::<ShadowSetting>.after(apply_setting::<ShadowDistanceSetting>),
        );

        add_enum_setting::<ImposterSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<FogSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<BloomSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<DofSetting>(app, &mut settings, &mut schedule, &config);
        #[cfg(not(target_arch = "wasm32"))]
        add_enum_setting::<SsaoSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<OobSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<AaSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<AmbientSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<WindowSetting>(app, &mut settings, &mut schedule, &config);

        if !is_preview {
            add_int_setting::<LoadDistanceSetting>(app, &mut settings, &mut schedule, &config);
            add_int_setting::<UnloadDistanceSetting>(app, &mut settings, &mut schedule, &config);
        }

        add_enum_setting::<FpsTargetSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<SceneThreadsSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<MaxAvatarsSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<MasterVolumeSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<SceneVolumeSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<VoiceVolumeSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<SystemVolumeSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<AvatarVolumeSetting>(app, &mut settings, &mut schedule, &config);

        add_enum_setting::<ConstrainUiSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<RunSpeedSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<WalkSpeedSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<FrictionSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<JumpSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<GravitySetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<FallSpeedSetting>(app, &mut settings, &mut schedule, &config);

        add_int_setting::<PointerSensitivitySetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<CameraZoomSensitivitySetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<ScrollSensitivitySetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<MovementSensitivitySetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<CameraSensitivitySetting>(app, &mut settings, &mut schedule, &config);

        add_int_setting::<VideoThreadsSetting>(app, &mut settings, &mut schedule, &config);
        add_int_setting::<MaxDownloadsSetting>(app, &mut settings, &mut schedule, &config);
        add_enum_setting::<CacheSizeSetting>(app, &mut settings, &mut schedule, &config);

        app.insert_resource(settings);
        app.insert_resource(ApplyAppSettingsSchedule(schedule));
        app.init_resource::<ActiveCameras>();
        app.add_systems(
            Update,
            (
                record_cameras,
                apply_settings.run_if(|config: Res<AppConfig>| config.is_changed()),
            )
                .chain(),
        );
    }
}

pub enum SettingCategory {
    Gameplay,
    Graphics,
    Audio,
    Performance,
    Controls,
}

impl Display for SettingCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SettingCategory::Gameplay => "Gameplay",
            SettingCategory::Graphics => "Graphics",
            SettingCategory::Audio => "Audio",
            SettingCategory::Performance => "Performance",
            SettingCategory::Controls => "Controls",
        })
    }
}

pub trait AppSetting: Eq + 'static {
    type Param: SystemParam + 'static;
    fn title() -> String;
    fn description(&self) -> String;
    fn category() -> SettingCategory;
    fn load(config: &AppConfig) -> Self;
    fn save(&self, config: &mut AppConfig);
    fn apply(
        &self,
        param: SystemParamItem<Self::Param>,
        mut commands: Commands,
        cameras: &HashSet<Entity>,
    ) {
        for &cam in cameras {
            self.apply_to_camera(&param, commands.reborrow(), cam)
        }
    }
    fn apply_to_camera(
        &self,
        _param: &SystemParamItem<Self::Param>,
        _commands: Commands,
        _camera_entity: Entity,
    ) {
    }
}

pub trait EnumAppSetting: AppSetting + Sized + std::fmt::Debug {
    fn variants() -> Vec<Self>;
    fn name(&self) -> String;
}

pub trait IntAppSetting: AppSetting + Sized + std::fmt::Debug {
    fn from_int(value: i32) -> Self;
    fn value(&self) -> i32;
    fn min() -> i32;
    fn max() -> i32;
    fn scale() -> f32 {
        1.0
    }
    fn display(&self) -> String {
        format!("{}", self.value())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NamedVariant {
    name: String,
    description: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    pub min_value: f32,
    pub max_value: f32,
    pub named_variants: Vec<NamedVariant>,
    pub step_size: f32,
    pub value: f32,
    pub default: f32,
}

pub struct Setting {
    pub info: SettingInfo,
    apply: Option<
        Box<dyn Fn(&mut AppConfig, f32) -> Result<f32, anyhow::Error> + Send + Sync + 'static>,
    >,
}

#[derive(Resource)]
pub struct Settings {
    pub settings: Vec<Setting>,
}

impl Settings {
    pub fn get(&self) -> Vec<SettingInfo> {
        self.settings.iter().map(|s| s.info.clone()).collect()
    }

    pub fn set_value(
        &mut self,
        config: &mut AppConfig,
        name: &str,
        value: f32,
    ) -> Result<(), anyhow::Error> {
        let apply = self
            .settings
            .iter_mut()
            .find(|s| s.info.name == name)
            .ok_or(anyhow!(format!("{name} not found")))?
            .apply
            .take()
            .unwrap();
        let res = (apply)(config, value)?;
        let setting = self
            .settings
            .iter_mut()
            .find(|s| s.info.name == name)
            .unwrap();

        setting.apply = Some(apply);
        setting.info.value = res;
        Ok(())
    }

    pub fn add_int_setting<S: IntAppSetting>(&mut self, config: &AppConfig) {
        let value = S::load(config);
        self.settings.push(Setting {
            info: SettingInfo {
                name: S::title(),
                category: S::category().to_string(),
                description: S::description(&value),
                min_value: (S::min() as f32 * S::scale()).min(S::max() as f32 * S::scale()),
                max_value: (S::min() as f32 * S::scale()).max(S::max() as f32 * S::scale()),
                named_variants: Default::default(),
                value: value.value() as f32 * S::scale(),
                default: S::load(&AppConfig::default()).value() as f32 * S::scale(),
                step_size: S::scale().abs(),
            },
            apply: Some(Box::new(
                |config: &mut AppConfig, value: f32| -> Result<f32, anyhow::Error> {
                    let new_value = S::from_int((value / S::scale()) as i32);
                    new_value.save(config);
                    Ok(new_value.value() as f32 * S::scale())
                },
            )),
        });
    }

    pub fn add_enum_setting<S: EnumAppSetting>(&mut self, config: &AppConfig) {
        let value = S::load(config);
        let index = S::variants()
            .iter()
            .enumerate()
            .find(|(_, s)| **s == value)
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        let default_value = S::load(&AppConfig::default());
        let default_index = S::variants()
            .iter()
            .enumerate()
            .find(|(_, s)| **s == default_value)
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        self.settings.push(Setting {
            info: SettingInfo {
                name: S::title(),
                category: S::category().to_string(),
                description: S::description(&value),
                min_value: 0.0,
                max_value: S::variants().len() as f32,
                named_variants: S::variants()
                    .into_iter()
                    .map(|v| NamedVariant {
                        name: v.name(),
                        description: v.description(),
                    })
                    .collect(),
                value: index as f32,
                default: default_index as f32,
                step_size: 1.0,
            },
            apply: Some(Box::new(
                |config: &mut AppConfig, value: f32| -> Result<f32, anyhow::Error> {
                    S::variants()
                        .get(value as usize)
                        .ok_or(anyhow::anyhow!("invalid variant index"))?
                        .save(config);
                    Ok(value as usize as f32)
                },
            )),
        });
    }
}

fn send_settings(mut ev: EventReader<SystemApi>, settings: Res<Settings>) {
    for ev in ev.read() {
        if let SystemApi::GetSettings(sender) = ev {
            sender.send(settings.settings.iter().map(|s| s.info.clone()).collect());
        }
    }
}

fn receive_settings(
    mut ev: EventReader<SystemApi>,
    mut config: ResMut<AppConfig>,
    mut settings: ResMut<Settings>,
) {
    for ev in ev.read() {
        if let SystemApi::SetSetting(name, val) = ev {
            if let Err(e) = settings.set_value(&mut config, name, *val) {
                error!("Error setting {name}: {e}");
            }
        }
    }
}

#[derive(Resource)]
pub struct ApplyAppSettingsSchedule(Schedule);

#[derive(ScheduleLabel, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct ApplyAppSettingsLabel;

fn apply_settings(world: &mut World) {
    world.resource_scope(
        |world: &mut World, mut schedule: Mut<ApplyAppSettingsSchedule>| {
            schedule.0.run(world);
        },
    );

    platform::write_config_file(world.resource::<AppConfig>());
}

#[derive(Resource, Default)]
pub struct ActiveCameras(HashSet<Entity>);

impl ActiveCameras {
    pub fn get<'a>(&'a mut self, commands: &mut Commands) -> &'a HashSet<Entity> {
        self.0.retain(|c| commands.get_entity(*c).is_ok());
        &self.0
    }
}

fn apply_setting<S: AppSetting>(
    params: StaticSystemParam<S::Param>,
    config: Res<AppConfig>,
    mut commands: Commands,
    mut cameras: ResMut<ActiveCameras>,
) {
    let cameras = cameras.get(&mut commands);
    S::load(&config).apply(params.into_inner(), commands, cameras);
}

pub fn record_cameras(
    mut cameras: ResMut<ActiveCameras>,
    mut new_cams: EventReader<NewCameraEvent>,
) {
    for ev in new_cams.read() {
        cameras.0.insert(ev.0);
    }
}
