#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    prelude::*,
};
use tokio::sync::mpsc::Receiver;

use dcl_component::proto_components::kernel::comms::rfc4;

use crate::{
    profile::CurrentUserProfile, ChannelControl, NetworkMessage, Transport, TransportType,
};
use common::structs::MicState;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{participant::Participants, track::Tracks};

// main.rs or lib.rs

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(native::NativeLivekitPlugin);
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(web::WebLivekitPlugin);

        app.add_systems(Update, start_livekit);
        app.add_event::<StartLivekit>();
        app.init_resource::<MicState>();

        app.register_type::<Transporting>();
        app.register_type::<TransportedBy>();
    }
}

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    #[expect(dead_code, reason = "Will be used eventually")]
    address: String,
    receiver: Receiver<NetworkMessage>,
    control_receiver: Receiver<ChannelControl>,
    #[expect(dead_code, reason = "Will be used eventually")]
    retries: usize,
}

/// Indicates that the [`LivekitTransport`] is connected.
#[derive(Component)]
#[component(on_insert = on_insert_connected)]
struct Connected;

/// Indicates that the [`LivekitTransport`] is reconnecting.
#[derive(Component)]
#[component(on_insert = on_insert_reconnecting)]
struct Reconnecting;

/// Indicates that the [`LivekitTransport`] is disconnected.
#[derive(Component)]
#[component(on_insert = on_insert_disconnected)]
struct Disconnected;

/// Entities connected through this transport.
///
/// Can be either participants, or their tracks.
#[derive(Reflect, Component)]
#[reflect(Component)]
#[relationship_target(relationship = TransportedBy)]
pub struct Transporting(Vec<Entity>);

/// Transport this entity is connected through.
///
/// This can be either participants, or their tracks.
#[derive(Reflect, Component)]
#[reflect(Component)]
#[relationship(relationship_target = Transporting)]
pub struct TransportedBy(Entity);

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
            Name::new("LivekitTransport"),
            Transport {
                transport_type: TransportType::Livekit,
                sender,
                control: Some(control_sender),
                foreign_aliases: Default::default(),
            },
            LivekitTransport::build_transport(ev.address.to_owned(), receiver, control_receiver),
        ));
    }
}

/// Hook to remove [`Reconnecting`] and [`Disconnected`]
/// when [`Connected`] is inserted.
fn on_insert_connected(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Reconnecting, Disconnected)>();
}

/// Hook to remove [`Connected`] and [`Disconnected`]
/// when [`Reconnecting`] is inserted.
fn on_insert_reconnecting(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Connected, Disconnected)>();
}

/// Hook to remove [`Connected`] and [`Reconnecting`]
/// when [`Disconnected`] is inserted.
fn on_insert_disconnected(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Connected, Reconnecting)>();
}
