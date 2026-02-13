use bevy::{
    ecs::{component::ComponentIdFor, entity::EntityHashSet},
    prelude::*,
};
use bevy_console::{AddConsoleCommand, ConsoleCommand};
use common::structs::{PrimaryPlayerRes, SceneTime, TimeOfDay};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{PbSkyboxTime, TransitionMode},
    SceneComponentId,
};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, ContainingScene,
};

/// One hour in seconds
const ONE_HOUR: f32 = 60. * 60.;
/// One hour in seconds using u32
const ONE_HOUR_U32: u32 = 60 * 60;
/// 24 hours in seconds
const TWENTY_FOUR_HOURS: f32 = 24. * ONE_HOUR;
/// How many hours per second are advanced during a [`TimeSkip`]
const HOURS_PER_SECOND: f32 = 12. * ONE_HOUR;

#[derive(Component)]
struct TimeKeeper;

#[derive(Component)]
struct RunningClock {
    /// secs since midnight
    pub time: f32,
    pub speed: f32,
}

#[derive(Resource)]
struct TimeSkip {
    start: f32,
    end: f32,
    progress: f32,
    easing: EaseFunction,
}

#[derive(Component)]
struct SceneTimeSource(Entity);

#[derive(Component, Deref)]
#[component(immutable)]
struct SkyboxTime(PbSkyboxTime);

impl From<PbSkyboxTime> for SkyboxTime {
    fn from(value: PbSkyboxTime) -> Self {
        Self(value)
    }
}

#[derive(Component)]
struct SkyboxTimeSource(Entity);

pub struct DayNightPlugin;

impl Plugin for DayNightPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbSkyboxTime, SkyboxTime>(
            SceneComponentId::SKYBOX_TIME,
            ComponentPosition::RootOnly,
        );

        app.add_systems(Startup, start_clock);
        app.add_systems(
            First,
            (
                (
                    update_running_clock,
                    (fetch_time_from_skybox, fetch_time_from_scene).chain(),
                ),
                push_time_of_day_from_time_skip.run_if(resource_exists::<TimeSkip>),
                push_time_of_day_from_running_clock.run_if(not(resource_exists::<TimeSkip>)),
            )
                .chain()
                .after(bevy::time::TimeSystem),
        );

        app.add_console_command::<TimeOfDayConsoleCommand, _>(timeofday_console_command);
    }
}

fn start_clock(mut commands: Commands) {
    let time = 10.0 * 3600.0;
    commands.insert_resource(TimeOfDay { time });
    commands
        .spawn((
            TimeKeeper,
            RunningClock { time, speed: 12.0 },
            SceneTime { time: 0. },
        ))
        .observe(check_new_scene_time)
        .observe(check_new_skybox_time);
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn fetch_time_from_scene(
    mut commands: Commands,
    primary_player: Res<PrimaryPlayerRes>,
    containing_scene: ContainingScene,
    scenes: Query<(Entity, &RendererSceneContext, &SceneTime)>,
    time_keeper: Single<(Entity, Option<&SceneTime>, Option<&SceneTimeSource>), With<TimeKeeper>>,
) {
    let containing_primary_player = containing_scene.get(primary_player.0);
    let maybe_scene_time = scenes
        .iter_many_unique(EntityHashSet::from_iter(containing_primary_player))
        .min_by_key(|(_, renderer_scene_context, _)| {
            (
                renderer_scene_context.is_portable,
                renderer_scene_context.start_tick,
            )
        })
        .map(|(entity, _, scene_time)| (entity, scene_time));

    let (time_keeper, time_keeper_scene_time, scene_time_source) = time_keeper.into_inner();
    if let Some((entity, scene_time)) = maybe_scene_time {
        if scene_time_source
            .filter(|source| source.0 == entity)
            .is_none()
        {
            debug!("Now using time from scene {}.", entity);
            commands.entity(time_keeper).insert((
                SceneTime {
                    time: scene_time.time,
                },
                SceneTimeSource(entity),
            ));
        }
    } else if time_keeper_scene_time.is_some() {
        debug!("No longer using SceneTime.");
        commands
            .entity(time_keeper)
            .try_remove::<(SceneTime, SceneTimeSource)>();
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn fetch_time_from_skybox(
    mut commands: Commands,
    primary_player: Res<PrimaryPlayerRes>,
    containing_scene: ContainingScene,
    scenes: Query<(Entity, &RendererSceneContext, &SkyboxTime)>,
    time_keeper: Single<(Entity, Option<&SkyboxTime>, Option<&SkyboxTimeSource>), With<TimeKeeper>>,
    scene_time_component_id: ComponentIdFor<SceneTime>,
) {
    let containing_primary_player = containing_scene.get(primary_player.0);
    let maybe_skybox_time = scenes
        .iter_many_unique(EntityHashSet::from_iter(containing_primary_player))
        .min_by_key(|(_, renderer_scene_context, _)| {
            (
                renderer_scene_context.is_portable,
                renderer_scene_context.start_tick,
            )
        })
        .map(|(entity, _, skybox_time)| (entity, skybox_time));

    let (time_keeper, time_keeper_skybox_time, skybox_time_source) = time_keeper.into_inner();
    if let Some((entity, skybox_time)) = maybe_skybox_time {
        if skybox_time_source
            .filter(|source| source.0 == entity)
            .is_none()
        {
            debug!("Now using skybox time from scene {}.", entity);
            commands.entity(time_keeper).insert((
                SkyboxTime(PbSkyboxTime {
                    fixed_time: skybox_time.fixed_time,
                    transition_mode: skybox_time.transition_mode,
                }),
                SkyboxTimeSource(entity),
            ));
        }
    } else if time_keeper_skybox_time.is_some() {
        debug!("No longer using SkyboxTime.");
        commands
            .entity(time_keeper)
            .try_remove::<(SkyboxTime, SkyboxTimeSource)>();
        commands.trigger_targets(OnInsert, (time_keeper, *scene_time_component_id));
    }
}

fn update_running_clock(
    mut time_keeper: Single<&mut RunningClock, With<TimeKeeper>>,
    time: Res<Time>,
) {
    let speed = time_keeper.speed;
    time_keeper.time += time.delta_secs() * speed;
    time_keeper.time %= TWENTY_FOUR_HOURS;
    if time_keeper.time < 0.0 {
        time_keeper.time += TWENTY_FOUR_HOURS;
    }
}

fn push_time_of_day_from_time_skip(
    mut commands: Commands,
    mut time_skip: ResMut<TimeSkip>,
    mut time_of_day: ResMut<TimeOfDay>,
    time: Res<Time>,
) {
    trace!("Pushing time from TimeSkip");
    let delta = time.delta_secs();
    let amount_to_skip = (time_skip.end - time_skip.start).abs();
    let duration = amount_to_skip / HOURS_PER_SECOND;
    time_skip.progress += delta / duration;

    time_of_day.time = time_skip.start.lerp(
        time_skip.end,
        time_skip.easing.sample_clamped(time_skip.progress),
    ) % TWENTY_FOUR_HOURS;

    if time_skip.progress >= 1. {
        commands.remove_resource::<TimeSkip>();
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn push_time_of_day_from_running_clock(
    time_keeper: Single<&RunningClock, (With<TimeKeeper>, Without<SceneTime>, Without<SkyboxTime>)>,
    mut time_of_day: ResMut<TimeOfDay>,
) {
    trace!("Pushing time from RunningClock");
    let running_clock = time_keeper.into_inner();
    time_of_day.time = running_clock.time;
}

fn check_new_scene_time(
    _trigger: Trigger<OnInsert, SceneTime>,
    mut commands: Commands,
    time_keeper: Single<&SceneTime, (With<TimeKeeper>, Without<SkyboxTime>)>,
    mut time_of_day: ResMut<TimeOfDay>,
) {
    let scene_time = time_keeper.into_inner();
    if (scene_time.time - time_of_day.time).abs() < ONE_HOUR {
        time_of_day.time = scene_time.time;
    } else {
        let end = if scene_time.time > time_of_day.time {
            scene_time.time
        } else {
            scene_time.time + TWENTY_FOUR_HOURS
        };
        commands.insert_resource(TimeSkip {
            start: time_of_day.time,
            end,
            progress: 0.,
            easing: EaseFunction::SmoothStep,
        });
    }
}

fn check_new_skybox_time(
    _trigger: Trigger<OnInsert, SkyboxTime>,
    mut commands: Commands,
    time_keeper: Single<&SkyboxTime, With<TimeKeeper>>,
    mut time_of_day: ResMut<TimeOfDay>,
) {
    let skybox_time = time_keeper.into_inner();
    if (skybox_time.fixed_time as f32 - time_of_day.time).abs() < ONE_HOUR {
        time_of_day.time = skybox_time.fixed_time as f32;
    } else {
        let end = match (
            skybox_time.fixed_time as f32 > time_of_day.time,
            skybox_time.transition_mode(),
        ) {
            (true, TransitionMode::TmForward) => skybox_time.fixed_time as f32,
            (false, TransitionMode::TmForward) => skybox_time.fixed_time as f32 + TWENTY_FOUR_HOURS,
            (true, TransitionMode::TmBackward) => skybox_time.fixed_time as f32 - TWENTY_FOUR_HOURS,
            (false, TransitionMode::TmBackward) => skybox_time.fixed_time as f32,
        };
        commands.insert_resource(TimeSkip {
            start: time_of_day.time,
            end,
            progress: 0.,
            easing: EaseFunction::SmoothStep,
        });
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/time")]
pub struct TimeOfDayConsoleCommand {
    pub time: Option<f32>,
    pub speed: Option<f32>,
}

fn timeofday_console_command(
    mut input: ConsoleCommand<TimeOfDayConsoleCommand>,
    mut commands: Commands,
    time_keeper: Single<(&mut RunningClock, Has<SceneTime>), With<TimeKeeper>>,
) {
    if let Some(Ok(command)) = input.take() {
        let (mut running_clock, has_scene_time) = time_keeper.into_inner();
        let old_time = running_clock.time;
        if let Some(hours) = command.time {
            running_clock.time = (hours * ONE_HOUR) % TWENTY_FOUR_HOURS;
        }
        if let Some(speed) = command.speed {
            running_clock.speed = speed;
        }

        if (running_clock.time - old_time).abs() > ONE_HOUR {
            let end = if running_clock.time > old_time {
                running_clock.time
            } else {
                running_clock.time + TWENTY_FOUR_HOURS
            };
            if !has_scene_time {
                commands.insert_resource(TimeSkip {
                    start: old_time,
                    end,
                    progress: 0.,
                    easing: EaseFunction::SmoothStep,
                });
            }
        }

        input.reply_ok(format!(
            "time {}:{} -> {}:{}, speed {} (elapsed: {})",
            (old_time as u32 / ONE_HOUR_U32),
            old_time as u32 % ONE_HOUR_U32 / 60,
            (running_clock.time as u32 / ONE_HOUR_U32),
            running_clock.time as u32 % ONE_HOUR_U32 / 60,
            running_clock.speed,
            running_clock.time
        ));
    }
}
