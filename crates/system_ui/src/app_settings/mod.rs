use bevy::{
    ecs::{
        schedule::ScheduleLabel,
        system::{StaticSystemParam, SystemParam, SystemParamItem},
    },
    prelude::*,
    ui::RelativeCursorPosition,
};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::structs::{
    AaSetting, AppConfig, BloomSetting, FogSetting, ShadowSetting, SsaoSetting, WindowSetting,
};
use ui_core::ui_actions::{Click, ClickRepeat, HoverEnter, On, UiCaller};

use crate::profile::{SettingsDialog, SettingsTab};

use self::{
    ambient_brightness_setting::AmbientSetting,
    constrain_ui::ConstrainUiSetting,
    frame_rate::FpsTargetSetting,
    load_distance::{LoadDistanceSetting, UnloadDistanceSetting},
    max_avatars::MaxAvatarsSetting,
    oob_setting::OobSetting,
    player_settings::{
        FallSpeedSetting, FrictionSetting, GravitySetting, JumpSetting, RunSpeedSetting,
    },
    scene_threads::SceneThreadsSetting,
    shadow_settings::ShadowDistanceSetting,
    video_threads::VideoThreadsSetting,
    volume_settings::{
        MasterVolumeSetting, SceneVolumeSetting, SystemVolumeSetting, VoiceVolumeSetting,
    },
};

// use self::window_settings::{set_resolutions, MonitorResolutions};

pub struct AppSettingsPlugin;

mod aa_settings;
pub mod ambient_brightness_setting;
mod bloom_settings;
pub mod constrain_ui;
pub mod fog_settings;
pub mod frame_rate;
pub mod load_distance;
pub mod max_avatars;
mod oob_setting;
pub mod player_settings;
pub mod scene_threads;
mod shadow_settings;
pub mod ssao_setting;
pub mod video_threads;
pub mod volume_settings;
pub mod window_settings;

impl Plugin for AppSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_app_settings_content);

        let mut apply_schedule = Schedule::new(ApplyAppSettingsLabel);

        apply_schedule.add_systems((
            apply_setting::<ShadowDistanceSetting>,
            apply_setting::<ShadowSetting>.after(apply_setting::<ShadowDistanceSetting>),
            apply_setting::<FogSetting>,
            apply_setting::<BloomSetting>,
            apply_setting::<SsaoSetting>,
            apply_setting::<OobSetting>,
            apply_setting::<AaSetting>,
            apply_setting::<AmbientSetting>,
            apply_setting::<WindowSetting>,
            apply_setting::<LoadDistanceSetting>,
            apply_setting::<UnloadDistanceSetting>,
            apply_setting::<FpsTargetSetting>,
            apply_setting::<SceneThreadsSetting>,
            apply_setting::<MaxAvatarsSetting>,
            apply_setting::<MasterVolumeSetting>,
            apply_setting::<SceneVolumeSetting>,
            apply_setting::<VoiceVolumeSetting>,
            apply_setting::<SystemVolumeSetting>,
            apply_setting::<ConstrainUiSetting>,
        ));
        apply_schedule.add_systems((
            apply_setting::<RunSpeedSetting>,
            apply_setting::<FrictionSetting>,
            apply_setting::<JumpSetting>,
            apply_setting::<GravitySetting>,
            apply_setting::<FallSpeedSetting>,
            apply_setting::<VideoThreadsSetting>,
        ));

        app.insert_resource(ApplyAppSettingsSchedule(apply_schedule));
        app.add_systems(
            Update,
            (
                apply_settings.run_if(|config: Res<AppConfig>| config.is_changed()),
                // set_resolutions,
            ),
        );
    }
}

#[derive(Component)]
pub struct AppSettingsDetail(pub AppConfig);

#[allow(clippy::type_complexity)]
fn set_app_settings_content(
    mut commands: Commands,
    dialog: Query<(Entity, Option<&AppSettingsDetail>), With<SettingsDialog>>,
    q: Query<(Entity, &SettingsTab), Changed<SettingsTab>>,
    current_settings: Res<AppConfig>,
    mut prev_tab: Local<Option<SettingsTab>>,
    dui: Res<DuiRegistry>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab) in q.iter() {
        let Ok((settings_entity, maybe_settings)) = dialog.get_single() else {
            return;
        };

        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Settings {
            return;
        }

        let config = match maybe_settings {
            Some(s) => s.0.clone(),
            None => {
                commands
                    .entity(settings_entity)
                    .insert(AppSettingsDetail(current_settings.clone()));
                current_settings.clone()
            }
        };

        commands.entity(ent).despawn_descendants();
        let components = commands
            .entity(ent)
            .apply_template(&dui, "settings-tab", DuiProps::new())
            .unwrap();

        let children = vec![
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Graphics Settings".to_owned()),
                )
                .unwrap()
                .root,
            WindowSetting::spawn_template(&mut commands, &dui, &config),
            // FullscreenResSetting::spawn_template(&mut commands, &dui, &config),
            AaSetting::spawn_template(&mut commands, &dui, &config),
            AmbientSetting::spawn_template(&mut commands, &dui, &config),
            ShadowSetting::spawn_template(&mut commands, &dui, &config),
            ShadowDistanceSetting::spawn_template(&mut commands, &dui, &config),
            FogSetting::spawn_template(&mut commands, &dui, &config),
            BloomSetting::spawn_template(&mut commands, &dui, &config),
            SsaoSetting::spawn_template(&mut commands, &dui, &config),
            OobSetting::spawn_template(&mut commands, &dui, &config),
            ConstrainUiSetting::spawn_template(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Performance Settings".to_owned()),
                )
                .unwrap()
                .root,
            LoadDistanceSetting::spawn_template(&mut commands, &dui, &config),
            UnloadDistanceSetting::spawn_template(&mut commands, &dui, &config),
            FpsTargetSetting::spawn_template(&mut commands, &dui, &config),
            SceneThreadsSetting::spawn_template(&mut commands, &dui, &config),
            VideoThreadsSetting::spawn_template(&mut commands, &dui, &config),
            MaxAvatarsSetting::spawn_template(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Audio Settings".to_owned()),
                )
                .unwrap()
                .root,
            MasterVolumeSetting::spawn_template(&mut commands, &dui, &config),
            SceneVolumeSetting::spawn_template(&mut commands, &dui, &config),
            VoiceVolumeSetting::spawn_template(&mut commands, &dui, &config),
            SystemVolumeSetting::spawn_template(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Player Dynamics Settings".to_owned()),
                )
                .unwrap()
                .root,
            RunSpeedSetting::spawn_template(&mut commands, &dui, &config),
            FrictionSetting::spawn_template(&mut commands, &dui, &config),
            JumpSetting::spawn_template(&mut commands, &dui, &config),
            GravitySetting::spawn_template(&mut commands, &dui, &config),
            FallSpeedSetting::spawn_template(&mut commands, &dui, &config),
        ];

        commands
            .entity(components.named("settings"))
            .push_children(&children);

        commands
            .entity(components.named("settings-description"))
            .insert(AppSettingDescription);
    }
}

pub trait AppSetting: Eq + 'static {
    type Param: SystemParam + 'static;
    fn title() -> String;
    fn description(&self) -> String;
    fn load(config: &AppConfig) -> Self;
    fn save(&self, config: &mut AppConfig);
    fn apply(&self, param: SystemParamItem<Self::Param>, commands: Commands);
    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity;
}

pub trait EnumAppSetting: AppSetting + Sized + std::fmt::Debug {
    type VParam: SystemParam + 'static;
    fn variants(param: SystemParamItem<Self::VParam>) -> Vec<Self>;
    fn name(&self) -> String;
}

pub trait IntAppSetting: AppSetting + Sized + std::fmt::Debug {
    fn from_int(value: i32) -> Self;
    fn value(&self) -> i32;
    fn min() -> i32;
    fn max() -> i32;
    fn display(&self) -> String {
        format!("{}", self.value())
    }
}

#[derive(Component)]
struct AppSettingDescription;

#[allow(clippy::too_many_arguments)]
fn bump_enum<S: EnumAppSetting, const I: isize>(
    mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
    params: StaticSystemParam<S::Param>,
    v_params: StaticSystemParam<S::VParam>,
    commands: Commands,
    caller: Res<UiCaller>,
    parents: Query<(&Parent, Option<&DuiEntities>)>,
    mut text: Query<&mut Text, Without<AppSettingDescription>>,
    mut description: Query<&mut Text, With<AppSettingDescription>>,
) {
    let mut variants = S::variants(v_params.into_inner());
    let (mut dialog, mut config) = q.single_mut();
    let config = &mut config.0;
    let current = S::load(config);
    let index = variants.iter().position(|v| v == &current).unwrap();
    let next =
        variants.remove(((index as isize + I) + variants.len() as isize) as usize % variants.len());
    S::save(&next, config);
    S::apply(&next, params.into_inner(), commands);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.map_or(true, |e| e.get_named("setting-label").is_none()) {
        (parent, entities) = parents.get(parent.get()).unwrap()
    }
    text.get_mut(entities.unwrap().named("setting-label"))
        .unwrap()
        .sections[0]
        .value = next.name();
    description.single_mut().sections[0].value = next.description();
    dialog.modified = true;
}

fn bump_int<S: IntAppSetting, const I: i32>(
    mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
    params: StaticSystemParam<S::Param>,
    commands: Commands,
    caller: Res<UiCaller>,
    parents: Query<(&Parent, Option<&DuiEntities>)>,
    mut style: Query<&mut Style>,
    mut text: Query<&mut Text, Without<AppSettingDescription>>,
) {
    let (mut dialog, mut config) = q.single_mut();
    let config = &mut config.0;
    let current = S::load(config).value();
    let next = S::from_int((current + I).clamp(S::min(), S::max()));
    S::save(&next, config);
    S::apply(&next, params.into_inner(), commands);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.map_or(true, |e| e.get_named("marker").is_none()) {
        (parent, entities) = parents.get(parent.get()).unwrap()
    }
    style
        .get_mut(entities.unwrap().named("marker"))
        .unwrap()
        .left =
        Val::Percent((next.value() - S::min()) as f32 / (S::max() - S::min()) as f32 * 100.0);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.map_or(true, |e| e.get_named("setting-label").is_none()) {
        (parent, entities) = parents.get(parent.get()).unwrap()
    }
    text.get_mut(entities.unwrap().named("setting-label"))
        .unwrap()
        .sections[0]
        .value = next.display();

    dialog.modified = true;
}

fn spawn_enum_setting_template<S: EnumAppSetting>(
    commands: &mut Commands,
    dui: &DuiRegistry,
    config: &AppConfig,
) -> Entity {
    let components = commands
        .spawn_template(
            dui,
            "enum-setting",
            DuiProps::new()
                .with_prop("title", S::title())
                .with_prop("label-initial", S::load(config).name())
                .with_prop("next", On::<Click>::new(bump_enum::<S, 1>))
                .with_prop("prev", On::<Click>::new(bump_enum::<S, -1>)),
        )
        .unwrap();

    commands.entity(components.root).insert((
        Interaction::default(),
        On::<HoverEnter>::new(
            |q: Query<&AppSettingsDetail>,
             mut description: Query<&mut Text, With<AppSettingDescription>>| {
                let value = S::load(&q.single().0);
                description.single_mut().sections[0].value = value.description();
            },
        ),
    ));

    components.root
}

fn spawn_int_setting_template<S: IntAppSetting>(
    commands: &mut Commands,
    dui: &DuiRegistry,
    config: &AppConfig,
) -> Entity {
    let initial_offset = (S::load(config).value() - S::min()) as f32 / (S::max() - S::min()) as f32;

    let components = commands
        .spawn_template(
            dui,
            "int-setting",
            DuiProps::new()
                .with_prop("title", S::title())
                .with_prop("initial-offset", format!("{}%", initial_offset * 100.0))
                .with_prop("label-initial", S::load(config).display())
                .with_prop("next", On::<ClickRepeat>::new(bump_int::<S, 1>))
                .with_prop("prev", On::<ClickRepeat>::new(bump_int::<S, -1>)),
        )
        .unwrap();

    commands.entity(components.root).insert((
        Interaction::default(),
        On::<HoverEnter>::new(
            |q: Query<&AppSettingsDetail>,
             mut description: Query<&mut Text, With<AppSettingDescription>>| {
                let value = S::load(&q.single().0);
                description.single_mut().sections[0].value = value.description();
            },
        ),
    ));

    commands.entity(components.named("container")).insert((
        Interaction::default(),
        RelativeCursorPosition::default(),
        On::<ClickRepeat>::new(
            |caller: Res<UiCaller>,
             cursor: Query<&RelativeCursorPosition>,
             mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
             params: StaticSystemParam<S::Param>,
             commands: Commands,
             parents: Query<(&Parent, Option<&DuiEntities>)>,
             mut style: Query<&mut Style>,
             mut text: Query<&mut Text, Without<AppSettingDescription>>| {
                let Some(pos) = cursor.get(caller.0).ok().and_then(|rcp| rcp.normalized) else {
                    return;
                };

                let new = S::min() + ((S::max() - S::min()) as f32 * pos.x.clamp(0.0, 1.0)) as i32;
                let next = S::from_int(new);

                let (mut dialog, mut config) = q.single_mut();
                let config = &mut config.0;
                S::save(&next, config);
                S::apply(&next, params.into_inner(), commands);

                let (mut parent, mut entities) = parents.get(caller.0).unwrap();
                while entities.map_or(true, |e| e.get_named("marker").is_none()) {
                    (parent, entities) = parents.get(parent.get()).unwrap()
                }
                style
                    .get_mut(entities.unwrap().named("marker"))
                    .unwrap()
                    .left = Val::Percent(
                    (next.value() - S::min()) as f32 / (S::max() - S::min()) as f32 * 100.0,
                );

                let (mut parent, mut entities) = parents.get(caller.0).unwrap();
                while entities.map_or(true, |e| e.get_named("setting-label").is_none()) {
                    (parent, entities) = parents.get(parent.get()).unwrap()
                }
                text.get_mut(entities.unwrap().named("setting-label"))
                    .unwrap()
                    .sections[0]
                    .value = next.display();

                dialog.modified = true;
            },
        ),
    ));

    components.root
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

    std::fs::write(
        "config.json",
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
