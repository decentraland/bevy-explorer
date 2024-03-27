use bevy::{
    ecs::{
        schedule::ScheduleLabel,
        system::{StaticSystemParam, SystemParam, SystemParamItem},
    },
    prelude::*,
};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::structs::{
    AaSetting, AppConfig, BloomSetting, FogSetting, ShadowSetting, WindowSetting,
};
use ui_core::ui_actions::{Click, HoverEnter, On, UiCaller};

use crate::profile::{SettingsDialog, SettingsTab};

use self::oob_setting::OobSetting;

// use self::window_settings::{set_resolutions, MonitorResolutions};

pub struct AppSettingsPlugin;

mod aa_settings;
mod bloom_settings;
pub mod fog_settings;
mod oob_setting;
mod shadow_settings;
pub mod window_settings;

impl Plugin for AppSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_app_settings_content);

        let mut apply_schedule = Schedule::new(ApplyAppSettingsLabel);

        apply_schedule.add_systems((
            apply_setting::<ShadowSetting>,
            apply_setting::<FogSetting>,
            apply_setting::<BloomSetting>,
            apply_setting::<OobSetting>,
            apply_setting::<AaSetting>,
            apply_setting::<WindowSetting>,
            // apply_setting::<FullscreenResSetting>.after(apply_setting::<WindowSetting>),
        ));

        app.insert_resource(ApplyAppSettingsSchedule(apply_schedule));
        // app.init_resource::<MonitorResolutions>();
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
            WindowSetting::spawn_template(&mut commands, &dui, &config),
            // FullscreenResSetting::spawn_template(&mut commands, &dui, &config),
            AaSetting::spawn_template(&mut commands, &dui, &config),
            ShadowSetting::spawn_template(&mut commands, &dui, &config),
            FogSetting::spawn_template(&mut commands, &dui, &config),
            BloomSetting::spawn_template(&mut commands, &dui, &config),
            OobSetting::spawn_template(&mut commands, &dui, &config),
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

#[derive(Component)]
struct AppSettingDescription;

fn spawn_enum_setting_template<S: EnumAppSetting>(
    commands: &mut Commands,
    dui: &DuiRegistry,
    config: &AppConfig,
) -> Entity {
    let change = |offset: isize| -> On<Click> {
        On::<Click>::new(
            move |mut q: Query<(&mut SettingsDialog, &mut AppSettingsDetail)>,
                  params: StaticSystemParam<S::Param>,
                  v_params: StaticSystemParam<S::VParam>,
                  commands: Commands,
                  caller: Res<UiCaller>,
                  parents: Query<(&Parent, Option<&DuiEntities>)>,
                  mut text: Query<&mut Text, Without<AppSettingDescription>>,
                  mut description: Query<&mut Text, With<AppSettingDescription>>| {
                let mut variants = S::variants(v_params.into_inner());
                let (mut dialog, mut config) = q.single_mut();
                let config = &mut config.0;
                let current = S::load(config);
                let index = variants.iter().position(|v| v == &current).unwrap();
                let next = variants.remove(
                    ((index as isize + offset) + variants.len() as isize) as usize % variants.len(),
                );
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
            },
        )
    };

    let components = commands
        .spawn_template(
            dui,
            "enum-setting",
            DuiProps::new()
                .with_prop("title", S::title())
                .with_prop("label-initial", S::load(config).name())
                .with_prop("next", change(1))
                .with_prop("prev", change(-1)),
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
