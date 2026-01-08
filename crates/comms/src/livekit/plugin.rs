use bevy::{ecs::relationship::Relationship, prelude::*};
use dcl_component::proto_components::kernel::comms::rfc4;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    global_crdt::NetworkUpdate,
    livekit::{
        mic::MicPlugin, participant::plugin::LivekitParticipantPlugin,
        room::plugin::LivekitRoomPlugin, runtime::LivekitRuntimePlugin,
        track::plugin::LivekitTrackPlugin, LivekitChannelControl, LivekitNetworkMessage,
        LivekitRuntime, LivekitTransport, StartLivekit, StreamBroadcast, StreamImage, StreamViewer,
    },
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerUpdateTasks>();

        app.add_plugins(MicPlugin);
        app.add_plugins(LivekitRuntimePlugin);
        app.add_plugins(LivekitRoomPlugin);
        app.add_plugins(LivekitParticipantPlugin);
        app.add_plugins(LivekitTrackPlugin);

        app.add_systems(
            Update,
            (
                start_livekit,
                verify_player_update_tasks,
                stream_viewer_without_stream_image,
                non_stream_viewer_with_stream_image,
            ),
        );
        app.add_event::<StartLivekit>();
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

fn stream_viewer_without_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<(Entity, &StreamViewer), Without<StreamImage>>,
    stream_broadcasts: Query<&StreamImage, With<StreamBroadcast>>,
) {
    for (entity, stream_viewer) in stream_viewers.into_inner() {
        let Ok(stream_image) = stream_broadcasts.get(stream_viewer.get()) else {
            error!("Invalid StreamBroadcast relationship.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        commands.entity(entity).insert(stream_image.clone());
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn non_stream_viewer_with_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<
        Entity,
        (
            Without<StreamViewer>,
            Without<StreamBroadcast>,
            With<StreamImage>,
        ),
    >,
) {
    for entity in stream_viewers.into_inner() {
        commands.entity(entity).remove::<StreamImage>();
    }
}
