use bevy::prelude::*;
use common::{
    debug_panic,
    structs::{AudioSettings, DisconnectReason, LivekitUpdate},
    util::ReportErr,
};
use dcl_component::proto_components::kernel::comms::rfc4;
use kira::{
    manager::{AudioManager, AudioManagerSettings, DefaultBackend},
    tween::Tween,
};
use system_bridge::SystemApi;
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(feature = "room_debug")]
use crate::livekit::room_debug::RoomDebugPlugin;
use crate::{
    global_crdt::NetworkUpdate,
    livekit::{
        mic::MicPlugin, participant::plugin::LivekitParticipantPlugin,
        room::plugin::LivekitRoomPlugin, runtime::LivekitRuntimePlugin,
        track::plugin::LivekitTrackPlugin, ConnectionAvailability, LivekitAudioManager,
        LivekitChannelControl, LivekitNetworkMessage, LivekitRuntime, LivekitSystemApiSenders,
        LivekitTransport, StartLivekit,
    },
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerUpdateTasks>();
        app.init_resource::<LivekitSystemApiSenders>();
        app.init_state::<ConnectionAvailability>();

        app.add_plugins(MicPlugin);
        app.add_plugins(LivekitRuntimePlugin);
        app.add_plugins(LivekitRoomPlugin);
        app.add_plugins(LivekitParticipantPlugin);
        app.add_plugins(LivekitTrackPlugin);

        app.add_systems(Update, (start_livekit, verify_player_update_tasks));
        app.add_systems(Startup, build_kira_audio_manager);
        app.add_systems(
            Update,
            respond_to_audio_settings_change.run_if(resource_exists_and_changed::<AudioSettings>),
        );
        app.add_systems(
            PostUpdate,
            (
                new_system_ai_senders,
                disconnect_reason.run_if(on_event::<DisconnectReason>),
                connection_availability_changed.run_if(state_changed::<ConnectionAvailability>),
            )
                .chain(),
        );

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
        let (control_sender, control_receiver) = tokio::sync::mpsc::channel(128);

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

fn verify_player_update_tasks(mut player_update_tasks: ResMut<PlayerUpdateTasks>) {
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
                        debug_panic!("Failed to send PlayerUpdate due to {err}.");
                    }
                }
                Err(err) => {
                    debug_panic!("Failed to pull PlayerUpdateTask due to '{err}'.");
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
            debug_panic!("Failed to livekit build AudioManager due to '{err}'.");
        }
    };
}

fn respond_to_audio_settings_change(
    mut livekit_audio_manager: ResMut<LivekitAudioManager>,
    audio_settings: Res<AudioSettings>,
) {
    livekit_audio_manager
        .main_track()
        .set_volume(audio_settings.scene() as f64, Tween::default());
}

fn new_system_ai_senders(
    mut event_reader: EventReader<SystemApi>,
    mut livekit_system_api_senders: ResMut<LivekitSystemApiSenders>,
) {
    for e in event_reader.read() {
        if let SystemApi::LivekitStatusStream(sx) = e {
            livekit_system_api_senders.push(sx.clone());
        }
    }
}

fn disconnect_reason(
    mut disconnect_reason: EventReader<DisconnectReason>,
    mut livekit_system_api_senders: ResMut<LivekitSystemApiSenders>,
) {
    for event in disconnect_reason.read() {
        for sender in livekit_system_api_senders.iter_mut() {
            sender
                .send(LivekitUpdate::DisconnectReason(*event))
                .report();
        }
    }
}

fn connection_availability_changed(
    connection_availability: Res<State<ConnectionAvailability>>,
    mut livekit_system_api_senders: ResMut<LivekitSystemApiSenders>,
) {
    let new_state = connection_availability.get();
    for sender in livekit_system_api_senders.iter_mut() {
        sender
            .send(LivekitUpdate::Availability(*new_state))
            .report();
    }
}
