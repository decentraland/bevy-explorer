use std::{
    fmt::Display,
    sync::{Arc, RwLock},
};

use ambient_brightness_setting::AmbientSetting;
use anyhow::anyhow;
use bevy::{
    app::{Plugin, Update},
    ecs::{
        schedule::ScheduleLabel,
        system::{StaticSystemParam, SystemParam, SystemParamItem},
    },
    prelude::*,
};
use common::{
    structs::{
        AaSetting, AppConfig, BloomSetting, FogSetting, ShadowSetting, SsaoSetting, WindowSetting,
    },
    util::config_file,
};
use constrain_ui::ConstrainUiSetting;
use despawn_workaround::DespawnWorkaroundSetting;
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

use crate::SystemApi;

pub mod aa_settings;
pub mod ambient_brightness_setting;
pub mod bloom_settings;
pub mod constrain_ui;
pub mod despawn_workaround;
pub mod fog_settings;
pub mod frame_rate;
pub mod load_distance;
pub mod max_avatars;
pub mod max_downloads;
pub mod oob_setting;
pub mod player_settings;
pub mod scene_threads;
pub mod shadow_settings;
pub mod ssao_setting;
pub mod video_threads;
pub mod volume_settings;
pub mod window_settings;

pub struct SettingBridgePlugin;

impl Plugin for SettingBridgePlugin {
    fn build(&self, app: &mut App) {
        fn add_int_setting<T: IntAppSetting>(settings: &mut Settings, schedule: &mut Schedule) {
            settings.add_int_setting::<T>();
            schedule.add_systems(apply_setting::<T>);
        }

        fn add_enum_setting<T: EnumAppSetting>(settings: &mut Settings, schedule: &mut Schedule) {
            settings.add_enum_setting::<T>();
            schedule.add_systems(apply_setting::<T>);
        }

        let config_copy = app.world().resource::<AppConfig>().clone();
        let mut settings = Settings {
            inner: Arc::new(RwLock::new(SettingsInner {
                settings: Vec::default(),
                config_copy,
                updated: false,
            })),
        };
        app.add_systems(Update, (Settings::sync_settings_object, send_settings));

        let mut schedule = Schedule::new(ApplyAppSettingsLabel);

        add_int_setting::<ShadowDistanceSetting>(&mut settings, &mut schedule);
        add_int_setting::<ShadowCasterCountSetting>(&mut settings, &mut schedule);

        // special case for ordering
        settings.add_enum_setting::<ShadowSetting>();
        schedule.add_systems(
            apply_setting::<ShadowSetting>.after(apply_setting::<ShadowDistanceSetting>),
        );

        add_enum_setting::<FogSetting>(&mut settings, &mut schedule);
        add_enum_setting::<BloomSetting>(&mut settings, &mut schedule);
        add_enum_setting::<SsaoSetting>(&mut settings, &mut schedule);
        add_enum_setting::<OobSetting>(&mut settings, &mut schedule);
        add_enum_setting::<AaSetting>(&mut settings, &mut schedule);
        add_int_setting::<AmbientSetting>(&mut settings, &mut schedule);
        add_enum_setting::<WindowSetting>(&mut settings, &mut schedule);
        add_int_setting::<LoadDistanceSetting>(&mut settings, &mut schedule);
        add_int_setting::<UnloadDistanceSetting>(&mut settings, &mut schedule);
        add_enum_setting::<FpsTargetSetting>(&mut settings, &mut schedule);
        add_int_setting::<SceneThreadsSetting>(&mut settings, &mut schedule);
        add_int_setting::<MaxAvatarsSetting>(&mut settings, &mut schedule);
        add_int_setting::<MasterVolumeSetting>(&mut settings, &mut schedule);
        add_int_setting::<SceneVolumeSetting>(&mut settings, &mut schedule);
        add_int_setting::<VoiceVolumeSetting>(&mut settings, &mut schedule);
        add_int_setting::<SystemVolumeSetting>(&mut settings, &mut schedule);
        add_int_setting::<AvatarVolumeSetting>(&mut settings, &mut schedule);

        add_enum_setting::<ConstrainUiSetting>(&mut settings, &mut schedule);
        add_int_setting::<RunSpeedSetting>(&mut settings, &mut schedule);
        add_int_setting::<WalkSpeedSetting>(&mut settings, &mut schedule);
        add_int_setting::<FrictionSetting>(&mut settings, &mut schedule);
        add_int_setting::<JumpSetting>(&mut settings, &mut schedule);
        add_int_setting::<GravitySetting>(&mut settings, &mut schedule);
        add_int_setting::<FallSpeedSetting>(&mut settings, &mut schedule);
        add_int_setting::<VideoThreadsSetting>(&mut settings, &mut schedule);
        add_int_setting::<MaxDownloadsSetting>(&mut settings, &mut schedule);
        add_enum_setting::<DespawnWorkaroundSetting>(&mut settings, &mut schedule);

        app.insert_resource(settings);
        app.insert_resource(ApplyAppSettingsSchedule(schedule));
        app.add_systems(
            Update,
            apply_settings.run_if(|config: Res<AppConfig>| config.is_changed()),
        );
    }
}

pub enum SettingCategory {
    Gameplay,
    Graphics,
    Audio,
    Performance,
}

impl Display for SettingCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SettingCategory::Gameplay => "Gameplay",
            SettingCategory::Graphics => "Graphics",
            SettingCategory::Audio => "Audio",
            SettingCategory::Performance => "Performance",
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
    fn apply(&self, param: SystemParamItem<Self::Param>, commands: Commands);
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
    pub value: f32,
}

pub struct Setting {
    pub info: SettingInfo,
    apply: Option<
        Box<dyn Fn(&mut AppConfig, f32) -> Result<(), anyhow::Error> + Send + Sync + 'static>,
    >,
}

pub struct SettingsInner {
    pub settings: Vec<Setting>,
    pub config_copy: AppConfig,
    pub updated: bool,
}

#[derive(Resource, Clone)]
pub struct Settings {
    pub inner: Arc<RwLock<SettingsInner>>,
}

impl Settings {
    pub fn get(&self) -> Vec<SettingInfo> {
        self.inner
            .read()
            .unwrap()
            .settings
            .iter()
            .map(|s| s.info.clone())
            .collect()
    }

    pub fn set_value(&self, name: &str, value: f32) -> Result<(), anyhow::Error> {
        let mut inner = self.inner.write().unwrap();
        let apply = inner
            .settings
            .iter_mut()
            .find(|s| s.info.name == name)
            .ok_or(anyhow!(format!("{name} not found")))?
            .apply
            .take()
            .unwrap();
        let res = (apply)(&mut inner.config_copy, value);
        inner
            .settings
            .iter_mut()
            .find(|s| s.info.name == name)
            .unwrap()
            .apply = Some(apply);
        inner.updated = true;
        res
    }

    pub fn add_int_setting<S: IntAppSetting>(&mut self) {
        let value = S::load(&self.inner.read().unwrap().config_copy);
        self.inner.write().unwrap().settings.push(Setting {
            info: SettingInfo {
                name: S::title(),
                category: S::category().to_string(),
                description: S::description(&value),
                min_value: S::min() as f32 * S::scale(),
                max_value: S::max() as f32 * S::scale(),
                named_variants: Default::default(),
                value: value.value() as f32 * S::scale(),
            },
            apply: Some(Box::new(
                |config: &mut AppConfig, value: f32| -> Result<(), anyhow::Error> {
                    S::from_int((value / S::scale()) as i32).save(config);
                    Ok(())
                },
            )),
        });
    }

    pub fn add_enum_setting<S: EnumAppSetting>(&mut self) {
        let value = S::load(&self.inner.read().unwrap().config_copy);
        let index = S::variants()
            .iter()
            .enumerate()
            .find(|(_, s)| **s == value)
            .unwrap()
            .0;
        self.inner.write().unwrap().settings.push(Setting {
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
            },
            apply: Some(Box::new(
                |config: &mut AppConfig, value: f32| -> Result<(), anyhow::Error> {
                    S::variants()
                        .get(value as usize)
                        .ok_or(anyhow::anyhow!("invalid variant index"))?
                        .save(config);
                    Ok(())
                },
            )),
        });
    }

    pub fn sync_settings_object(settings: Res<Self>, mut config: ResMut<AppConfig>) {
        if settings.inner.read().unwrap().updated {
            let mut write = settings.inner.write().unwrap();
            *config = write.config_copy.clone();
            write.updated = false;
        } else if config.is_changed() {
            let mut write = settings.inner.write().unwrap();
            write.config_copy = config.clone();
        }
    }
}

fn send_settings(mut ev: EventReader<SystemApi>, settings: Res<Settings>) {
    for ev in ev.read() {
        if let SystemApi::GetSettings(sender) = ev {
            sender.send(settings.clone());
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

    let config_file = config_file();
    if let Some(folder) = config_file.parent() {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(
        config_file,
        serde_json::to_string(world.resource::<AppConfig>()).unwrap(),
    )
    .unwrap();
}

fn apply_setting<S: AppSetting>(
    params: StaticSystemParam<S::Param>,
    config: Res<AppConfig>,
    commands: Commands,
) {
    S::load(&config).apply(params.into_inner(), commands);
}
