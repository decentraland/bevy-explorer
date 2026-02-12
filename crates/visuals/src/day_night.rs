use bevy::prelude::*;
use bevy_console::{AddConsoleCommand, ConsoleCommand};
use common::structs::{PrimaryUser, SceneTime, TimeOfDay};
use scene_runner::{ContainingScene, SceneEntity};

#[derive(Component)]
struct TimeKeeper;

#[derive(Component)]
struct RunningClock {
    /// secs since midnight
    pub time: f32,
    pub target_time: Option<f32>,
    pub speed: f32,
}

pub struct DayNightPlugin;

impl Plugin for DayNightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_clock);
        app.add_systems(
            First,
            (update_running_clock, push_time_of_day).after(bevy::time::TimeSystem),
        );

        app.add_console_command::<TimeOfDayConsoleCommand, _>(timeofday_console_command);
    }
}

fn start_clock(mut commands: Commands) {
    let time = 10.0 * 3600.0;
    commands.insert_resource(TimeOfDay { time });
    commands.spawn((
        TimeKeeper,
        RunningClock {
            time,
            target_time: None,
            speed: 12.0,
        },
    ));
}

fn update_running_clock(
    mut time_keeper: Single<&mut RunningClock, With<TimeKeeper>>,
    time: Res<Time>,
    mut t_delta: Local<f32>,
) {
    if let Some(target) = time_keeper.target_time {
        let initial_time = time_keeper.time;
        let seconds_diff = (target - time_keeper.time) % (24.0 * 3600.0);
        let seconds_to_travel = (seconds_diff + 12.0 * 3600.0) % (24.0 * 3600.0) - (12.0 * 3600.0);
        let unwrapped_target = initial_time + seconds_to_travel;

        time_keeper.time += *t_delta * time.delta_secs();

        const ACCEL: f32 = 4.0 * 3600.0;

        let total_change_min = *t_delta * 0.5 * (*t_delta / ACCEL);
        if (time_keeper.time + total_change_min - unwrapped_target).signum()
            == (time_keeper.time - unwrapped_target).signum()
        {
            *t_delta += time.delta_secs() * ACCEL * seconds_to_travel.signum();
        } else {
            // we overshoot at this speed, start slowing down
            *t_delta -= time.delta_secs() * ACCEL * seconds_to_travel.signum();
        }

        if (initial_time - target).signum() != (time_keeper.time - target).signum() {
            time_keeper.time = target;
            time_keeper.target_time = None;
            *t_delta = 0.0;
        }

        debug!("time: {initial_time}, target: {:?}, secs_to_travel: {seconds_to_travel}, t_delta: {}, final: {}", target, *t_delta, time_keeper.time);
    } else {
        let speed = time_keeper.speed;
        time_keeper.time += time.delta_secs() * speed;
        time_keeper.time %= 3600.0 * 24.0;
        if time_keeper.time < 0.0 {
            time_keeper.time += 3600.0 * 24.0;
        }
    }
}

fn push_time_of_day(
    time_keeper: Single<&RunningClock, With<TimeKeeper>>,
    maybe_player: Option<Single<Entity, With<PrimaryUser>>>,
    containing_scene: ContainingScene,
    scene_times: Query<&SceneTime, With<SceneEntity>>,
    mut time_of_day: ResMut<TimeOfDay>,
) {
    let running_clock = time_keeper.into_inner();

    let maybe_containing_scenes = maybe_player.map(|single| {
        let player = single.into_inner();
        containing_scene.get(player)
    });

    let maybe_scene_time = maybe_containing_scenes
        .as_ref()
        .and_then(|containing_scenes| {
            containing_scenes
                .iter()
                .find_map(|scene| scene_times.get(*scene).ok())
        });

    if let Some(scene_time) = maybe_scene_time {
        time_of_day.time = scene_time.time;
    } else {
        time_of_day.time = running_clock.time;
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
    mut time_keeper: Single<&mut RunningClock, With<TimeKeeper>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Some(hours) = command.time {
            time_keeper.target_time = Some(hours * 3600.0);
        }
        if let Some(speed) = command.speed {
            time_keeper.speed = speed;
        }

        let target = time_keeper.target_time.unwrap_or(time_keeper.time);
        input.reply_ok(format!(
            "time {}:{} -> {}:{}, speed {} (elapsed: {})",
            (time_keeper.time as u32 / 3600),
            time_keeper.time as u32 % 3600 / 60,
            (target as u32 / 3600),
            target as u32 % 3600 / 60,
            time_keeper.speed,
            time_keeper.time
        ));
    }
}
