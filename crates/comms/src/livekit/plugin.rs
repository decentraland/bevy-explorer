use bevy::prelude::*;
use dcl_component::proto_components::kernel::comms::rfc4;
use kira::manager::{AudioManager, AudioManagerSettings, DefaultBackend};
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(feature = "room_debug")]
use crate::livekit::room_debug::RoomDebugPlugin;
use crate::{
    global_crdt::NetworkUpdate,
    livekit::{
        mic::MicPlugin, participant::plugin::LivekitParticipantPlugin,
        room::plugin::LivekitRoomPlugin, runtime::LivekitRuntimePlugin,
        track::plugin::LivekitTrackPlugin, ConnectionAvailability, LivekitAudioManager,
        LivekitChannelControl, LivekitNetworkMessage, LivekitRuntime, LivekitTransport,
        StartLivekit,
    },
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerUpdateTasks>();
        app.init_state::<ConnectionAvailability>();

        app.add_plugins(MicPlugin);
        app.add_plugins(LivekitRuntimePlugin);
        app.add_plugins(LivekitRoomPlugin);
        app.add_plugins(LivekitParticipantPlugin);
        app.add_plugins(LivekitTrackPlugin);

        app.add_systems(Update, (start_livekit, verify_player_update_tasks));
        app.add_systems(Startup, build_kira_audio_manager);

        app.add_event::<StartLivekit>();

        #[cfg(feature = "room_debug")]
        app.add_plugins(RoomDebugPlugin);
    }
}

#[derive(Default, Resource, Deref, DerefMut)]
pub(super) struct PlayerUpdateTasks(Vec<PlayerUpdateTask>);

pub(super) struct PlayerUpdateTask {
    pub runtime: LivekitRuntime,
    pub task: JoinHandle<Result<(), mpsc::error::SendError<NetworkUpdate>>>,
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
                    if let Err(err) = res {
                        error!("Failed to send PlayerUpdate due to {err}.");
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

fn build_kira_audio_manager(mut commands: Commands) {
    match AudioManager::new(AudioManagerSettings::<DefaultBackend>::default()) {
        Ok(manager) => {
            debug!("Livekit AudioManager built.");
            commands.insert_resource(LivekitAudioManager { manager });
        }
        Err(err) => {
            error!("Failed to livekit build AudioManager due to '{err}'.");
            commands.send_event(AppExit::from_code(1));
        }
    };
}
