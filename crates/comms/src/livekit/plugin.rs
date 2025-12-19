use bevy::platform::sync::Arc;
use bevy::prelude::*;
use dcl_component::proto_components::kernel::comms::rfc4;
#[cfg(not(target_arch = "wasm32"))]
use livekit::RoomError;
use tokio::{runtime::Builder, sync::mpsc, task::JoinHandle};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{connect_livekit, RoomError};
use crate::{
    global_crdt::PlayerUpdate,
    livekit::{
        mic::MicPlugin, participant::plugin::LivekitParticipantPlugin,
        room::plugin::LivekitRoomPlugin, track::plugin::LivekitTrackPlugin, LivekitChannelControl,
        LivekitNetworkMessage, LivekitRuntime, LivekitTransport, StartLivekit,
    },
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerUpdateTasks>();
        app.init_resource::<RoomTasks>();

        app.add_plugins(MicPlugin);
        app.add_plugins(LivekitRoomPlugin);
        app.add_plugins(LivekitParticipantPlugin);
        app.add_plugins(LivekitTrackPlugin);

        app.add_systems(
            Update,
            (
                #[cfg(target_arch = "wasm32")]
                connect_livekit,
                start_livekit,
                verify_player_update_tasks,
                verify_room_tasks,
            ),
        );
        app.add_event::<StartLivekit>();
    }
}

#[derive(Default, Resource, Deref, DerefMut)]
pub(super) struct PlayerUpdateTasks(Vec<PlayerUpdateTask>);

pub(super) struct PlayerUpdateTask {
    pub runtime: LivekitRuntime,
    pub task: JoinHandle<Result<(), mpsc::error::SendError<PlayerUpdate>>>,
}

#[derive(Default, Resource, Deref, DerefMut)]
pub(super) struct RoomTasks(Vec<RoomTask>);

pub(super) struct RoomTask {
    pub runtime: LivekitRuntime,
    pub task: JoinHandle<Result<(), RoomError>>,
}

fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    for ev in room_events.read() {
        info!("starting livekit protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);
        let (control_sender, control_receiver) = tokio::sync::mpsc::channel(10);

        let Some(current_profile) = current_profile.profile.as_ref() else {
            return;
        };

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.version,
                },
            )),
            protocol_version: 100,
        };
        let _ = sender.try_send(NetworkMessage::reliable(&response));

        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        #[cfg(target_arch = "wasm32")]
        let runtime = Arc::new(
            Builder::new_current_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );

        commands.entity(ev.entity).try_insert((
            Transport {
                transport_type: TransportType::Livekit,
                sender,
                control: Some(control_sender),
                foreign_aliases: Default::default(),
            },
            LivekitTransport {
                address: ev.address.to_owned(),
                retries: 0,
            },
            LivekitChannelControl {
                receiver: control_receiver,
            },
            LivekitNetworkMessage { receiver },
            LivekitRuntime(runtime),
        ));
    }
}

fn verify_player_update_tasks(
    mut commands: Commands,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
) {
    let mut done = vec![];
    for (
        i,
        PlayerUpdateTask {
            runtime,
            ref mut task,
        },
    ) in player_update_tasks.iter_mut().enumerate()
    {
        if task.is_finished() {
            done.push(i);
            let res = runtime.block_on(task);
            match res {
                Ok(res) => {
                    if res.is_err() {
                        error!("Failed to send PlayerUpdate.");
                        commands.send_event(AppExit::from_code(1));
                        return;
                    }
                }
                Err(err) => {
                    error!("Failed to pull PlayerUpdateTask due to '{err}'.");
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
    }

    while let Some(i) = done.pop() {
        player_update_tasks.remove(i);
    }
}

fn verify_room_tasks(mut commands: Commands, mut network_message_tasks: ResMut<RoomTasks>) {
    let mut done = vec![];
    for (
        i,
        RoomTask {
            runtime,
            ref mut task,
        },
    ) in network_message_tasks.iter_mut().enumerate()
    {
        if task.is_finished() {
            done.push(i);
            let res = runtime.block_on(task);
            match res {
                Ok(res) => {
                    if res.is_err() {
                        error!("Failed to send PlayerUpdate.");
                        commands.send_event(AppExit::from_code(1));
                        return;
                    }
                }
                Err(err) => {
                    error!("Failed to pull PlayerUpdateTask due to '{err}'.");
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
    }

    while let Some(i) = done.pop() {
        network_message_tasks.remove(i);
    }
}
