pub mod client;

use bevy::prelude::*;
use client::{DirectChatMessage, SocialClientHandler};
use common::util::FireEventEx;
use dcl_component::proto_components::social::friendship_event_response;
use ethers_core::types::Address;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use wallet::Wallet;

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<FriendshipEvent>();
        app.add_event::<DirectChatEvent>();
        app.init_resource::<SocialClient>();
        app.add_systems(PostUpdate, |mut client: ResMut<SocialClient>| {
            if let Some(client) = client.0.as_mut() {
                client.update();
            }
        });
        app.add_systems(PostUpdate, init_social_client);
    }
}

pub fn init_social_client(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut social: ResMut<SocialClient>,
    mut friends: Local<Option<UnboundedReceiver<FriendshipEvent>>>,
    mut chats: Local<Option<UnboundedReceiver<DirectChatEvent>>>,
) {
    if wallet.is_changed() && wallet.address().is_some() {
        let (f_sx, f_rx) = unbounded_channel();
        let (c_sx, c_rx) = unbounded_channel();
        let client = SocialClientHandler::connect(
            wallet.clone(),
            move |f| {
                let _ = f_sx.send(FriendshipEvent(Some(f.clone())));
            },
            move |c| {
                let _ = c_sx.send(DirectChatEvent(c));
            },
        );
        social.0 = client;
        *friends = Some(f_rx);
        *chats = Some(c_rx);
    }

    while let Some(f) = friends.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.fire_event(f);
    }
    while let Some(c) = chats.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.fire_event(c);
    }
}

#[derive(Resource, Default)]
pub struct SocialClient(pub Option<SocialClientHandler>);

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum FriendshipState {
    NotFriends,
    SentRequest,
    RecdRequested,
    Friends,
    Error,
}

impl SocialClient {
    pub fn get_state(&self, address: Address) -> FriendshipState {
        let Some(client) = self.0.as_ref() else {
            return FriendshipState::Error;
        };
        if client.friends.contains(&address) {
            return FriendshipState::Friends;
        }
        if client.sent_requests.contains(&address) {
            return FriendshipState::SentRequest;
        }
        if client.received_requests.contains_key(&address) {
            return FriendshipState::RecdRequested;
        }
        FriendshipState::NotFriends
    }
}

#[derive(Event)]
pub struct FriendshipEvent(pub Option<friendship_event_response::Body>);

#[derive(Event)]
pub struct DirectChatEvent(pub DirectChatMessage);
