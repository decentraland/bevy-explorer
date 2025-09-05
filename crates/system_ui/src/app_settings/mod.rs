use bevy::{ecs::system::StaticSystemParam, prelude::*, ui::RelativeCursorPosition};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    structs::{
        AaSetting, AppConfig, BloomSetting, DofSetting, FogSetting, SettingsTab, ShadowSetting,
        SsaoSetting, WindowSetting,
    },
    util::TryPushChildrenEx,
};
use system_bridge::settings::{
    cache_size::CacheSizeSetting,
    imposter_settings::ImposterSetting,
    sensitivity::{
        CameraSensitivitySetting, CameraZoomSensitivitySetting, MovementSensitivitySetting,
        PointerSensitivitySetting, ScrollSensitivitySetting,
    },
    ActiveCameras, EnumAppSetting, IntAppSetting,
};
use ui_core::ui_actions::{Click, ClickRepeat, HoverEnter, On, UiCaller};

use crate::profile::SettingsDialog;

use system_bridge::settings::{
    ambient_brightness_setting::AmbientSetting,
    constrain_ui::ConstrainUiSetting,
    frame_rate::FpsTargetSetting,
    load_distance::{LoadDistanceSetting, UnloadDistanceSetting},
    max_avatars::MaxAvatarsSetting,
    max_downloads::MaxDownloadsSetting,
    oob_setting::OobSetting,
    player_settings::{
        FallSpeedSetting, FrictionSetting, GravitySetting, JumpSetting, RunSpeedSetting,
        WalkSpeedSetting,
    },
    scene_threads::SceneThreadsSetting,
    shadow_settings::ShadowCasterCountSetting,
    shadow_settings::ShadowDistanceSetting,
    video_threads::VideoThreadsSetting,
    volume_settings::{
        AvatarVolumeSetting, MasterVolumeSetting, SceneVolumeSetting, SystemVolumeSetting,
        VoiceVolumeSetting,
    },
};

// use self::window_settings::{set_resolutions, MonitorResolutions};

pub struct AppSettingsPlugin;

impl Plugin for AppSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_app_settings_content);
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
        let Ok((settings_entity, maybe_settings)) = dialog.single() else {
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

        commands.entity(ent).despawn_related::<Children>();
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
            spawn_enum_setting_template::<WindowSetting>(&mut commands, &dui, &config),
            // spawn_enum_setting_template::<FullscreenResSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<AaSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<AmbientSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<ShadowSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<ShadowDistanceSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<ShadowCasterCountSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<ImposterSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<FogSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<BloomSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<DofSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<SsaoSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<OobSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<ConstrainUiSetting>(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Performance Settings".to_owned()),
                )
                .unwrap()
                .root,
            spawn_int_setting_template::<LoadDistanceSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<UnloadDistanceSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<FpsTargetSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<SceneThreadsSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<VideoThreadsSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<MaxAvatarsSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<MaxDownloadsSetting>(&mut commands, &dui, &config),
            spawn_enum_setting_template::<CacheSizeSetting>(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Audio Settings".to_owned()),
                )
                .unwrap()
                .root,
            spawn_int_setting_template::<MasterVolumeSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<SceneVolumeSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<VoiceVolumeSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<SystemVolumeSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<AvatarVolumeSetting>(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Player Dynamics Settings".to_owned()),
                )
                .unwrap()
                .root,
            spawn_int_setting_template::<RunSpeedSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<WalkSpeedSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<FrictionSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<JumpSetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<GravitySetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<FallSpeedSetting>(&mut commands, &dui, &config),
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", "Control Sensitivity Settings".to_owned()),
                )
                .unwrap()
                .root,
            spawn_int_setting_template::<PointerSensitivitySetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<CameraZoomSensitivitySetting>(
                &mut commands,
                &dui,
                &config,
            ),
            spawn_int_setting_template::<ScrollSensitivitySetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<MovementSensitivitySetting>(&mut commands, &dui, &config),
            spawn_int_setting_template::<CameraSensitivitySetting>(&mut commands, &dui, &config),
        ];

        commands
            .entity(components.named("settings"))
            .try_push_children(&children);

        commands
            .entity(components.named("settings-description"))
            .insert(AppSettingDescription);
    }
}

#[derive(Component)]
struct AppSettingDescription;

#[allow(clippy::too_many_arguments)]
fn bump_enum<S: EnumAppSetting, const I: isize>(
    mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
    params: StaticSystemParam<S::Param>,
    mut commands: Commands,
    caller: Res<UiCaller>,
    parents: Query<(&ChildOf, Option<&DuiEntities>)>,
    mut text: Query<&mut Text, Without<AppSettingDescription>>,
    mut description: Query<&mut Text, With<AppSettingDescription>>,
    mut cameras: ResMut<ActiveCameras>,
) {
    let mut variants = S::variants();
    let (mut dialog, mut config) = q.single_mut().unwrap();
    let config = &mut config.0;
    let current = S::load(config);
    let index = variants.iter().position(|v| v == &current).unwrap();
    let next =
        variants.remove(((index as isize + I) + variants.len() as isize) as usize % variants.len());
    S::save(&next, config);
    let cameras = cameras.get(&mut commands);
    S::apply(&next, params.into_inner(), commands, cameras);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.is_none_or(|e| e.get_named("setting-label").is_none()) {
        (parent, entities) = parents.get(parent.parent()).unwrap()
    }
    text.get_mut(entities.unwrap().named("setting-label"))
        .unwrap()
        .0 = next.name();
    description.single_mut().unwrap().0 = next.description();
    dialog.modified = true;
}

#[allow(clippy::too_many_arguments)]
fn bump_int<S: IntAppSetting, const I: i32>(
    mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
    params: StaticSystemParam<S::Param>,
    mut commands: Commands,
    caller: Res<UiCaller>,
    parents: Query<(&ChildOf, Option<&DuiEntities>)>,
    mut style: Query<&mut Node>,
    mut text: Query<&mut Text, Without<AppSettingDescription>>,
    mut cameras: ResMut<ActiveCameras>,
) {
    let (mut dialog, mut config) = q.single_mut().unwrap();
    let config = &mut config.0;
    let current = S::load(config).value();
    let next = S::from_int((current + I).clamp(S::min(), S::max()));
    S::save(&next, config);
    let cameras = cameras.get(&mut commands);
    S::apply(&next, params.into_inner(), commands, cameras);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.is_none_or(|e| e.get_named("marker").is_none()) {
        (parent, entities) = parents.get(parent.parent()).unwrap()
    }
    style
        .get_mut(entities.unwrap().named("marker"))
        .unwrap()
        .left =
        Val::Percent((next.value() - S::min()) as f32 / (S::max() - S::min()) as f32 * 100.0);

    let (mut parent, mut entities) = parents.get(caller.0).unwrap();
    while entities.is_none_or(|e| e.get_named("setting-label").is_none()) {
        (parent, entities) = parents.get(parent.parent()).unwrap()
    }
    text.get_mut(entities.unwrap().named("setting-label"))
        .unwrap()
        .0 = next.display();

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
                let value = S::load(&q.single().unwrap().0);
                description.single_mut().unwrap().0 = value.description();
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
                let value = S::load(&q.single().unwrap().0);
                description.single_mut().unwrap().0 = value.description();
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
             mut commands: Commands,
             parents: Query<(&ChildOf, Option<&DuiEntities>)>,
             mut style: Query<&mut Node>,
             mut text: Query<&mut Text, Without<AppSettingDescription>>,
             mut cameras: ResMut<ActiveCameras>| {
                let Some(pos) = cursor.get(caller.0).ok().and_then(|rcp| rcp.normalized) else {
                    return;
                };

                let new = S::min() + ((S::max() - S::min()) as f32 * pos.x.clamp(0.0, 1.0)) as i32;
                let next = S::from_int(new);

                let (mut dialog, mut config) = q.single_mut().unwrap();
                let config = &mut config.0;
                S::save(&next, config);
                let cameras = cameras.get(&mut commands);
                S::apply(&next, params.into_inner(), commands, cameras);

                let (mut parent, mut entities) = parents.get(caller.0).unwrap();
                while entities.is_none_or(|e| e.get_named("marker").is_none()) {
                    (parent, entities) = parents.get(parent.parent()).unwrap()
                }
                style
                    .get_mut(entities.unwrap().named("marker"))
                    .unwrap()
                    .left = Val::Percent(
                    (next.value() - S::min()) as f32 / (S::max() - S::min()) as f32 * 100.0,
                );

                let (mut parent, mut entities) = parents.get(caller.0).unwrap();
                while entities.is_none_or(|e| e.get_named("setting-label").is_none()) {
                    (parent, entities) = parents.get(parent.parent()).unwrap()
                }
                text.get_mut(entities.unwrap().named("setting-label"))
                    .unwrap()
                    .0 = next.display();

                dialog.modified = true;
            },
        ),
    ));

    components.root
}
