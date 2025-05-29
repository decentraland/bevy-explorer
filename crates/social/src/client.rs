use anyhow::anyhow;
use bevy::{
    log::{debug, warn},
    utils::{HashMap, HashSet},
};
use common::util::AsH160;
use dcl_component::proto_components::social::{
    friendship_event_payload, friendship_event_response, request_events_response,
    subscribe_friendship_events_updates_response, users_response, AcceptPayload, CancelPayload,
    DeletePayload, FriendshipEventPayload, FriendshipsServiceClient,
    FriendshipsServiceClientDefinition, Payload, RejectPayload, RequestEvents, RequestPayload,
    RequestResponse, SubscribeFriendshipEventsUpdatesResponse, UpdateFriendshipPayload, User,
    Users,
};
use dcl_rpc::{client::RpcClient, transports::web_sockets::WebSocketTransport};
use ethers_core::types::Address;
use futures_util::{pin_mut, select, FutureExt};
use matrix_sdk::{
    config::SyncSettings,
    event_handler::Ctx,
    room::MessagesOptions,
    ruma::{
        api::client::{
            filter::{FilterDefinition, RoomEventFilter},
            receipt::create_receipt::v3::ReceiptType,
        },
        events::{
            receipt::ReceiptThread,
            room::message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            AnyMessageLikeEventContent, AnySyncTimelineEvent, MessageLikeEventType,
        },
        RoomOrAliasId, UserId,
    },
    Room, RoomMemberships,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, channel, Receiver, Sender, UnboundedReceiver, UnboundedSender};
use wallet::SimpleAuthChain;

use crate::DirectChatMessage;

#[derive(Serialize, Deserialize)]
struct SocialIdentifier {
    r#type: String,
    user: String,
}

impl SocialIdentifier {
    fn new(address: Address) -> Self {
        Self {
            r#type: "m.id.user".to_owned(),
            user: format!("{:#x}", address),
        }
    }
}

#[derive(Serialize)]
struct SocialLogin {
    auth_chain: SimpleAuthChain,
    identifier: SocialIdentifier,
    timestamp: String,
    r#type: String,
}

impl SocialLogin {
    async fn try_new(wallet: &wallet::Wallet) -> Result<Self, anyhow::Error> {
        let timestamp: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
        let timestamp = format!("{}", timestamp.timestamp_millis());

        let auth_chain = wallet
            .sign_message(timestamp.clone())
            .await
            .map_err(dbgerr)?;
        let identifier = SocialIdentifier::new(wallet.address().ok_or(anyhow!("not connected"))?);

        Ok(Self {
            auth_chain,
            identifier,
            timestamp,
            r#type: "m.login.decentraland".to_owned(),
        })
    }
}

enum FriendData {
    Init {
        sent_requests: HashSet<Address>,
        received_requests: HashMap<Address, Option<String>>,
        friends: HashSet<Address>,
    },
    Event(friendship_event_response::Body),
    Chat(DirectChatMessage),
}

enum FriendshipOutbound {
    FriendshipEvent(FriendshipEventPayload),
    ChatMessage(DirectChatMessage),
    HistoryRequest(Address, Sender<DirectChatMessage>),
}

pub struct SocialClientHandler {
    sender: UnboundedSender<FriendshipOutbound>,
    friendship_receiver: UnboundedReceiver<FriendData>,

    pub is_initialized: bool,
    pub sent_requests: HashSet<Address>,
    pub received_requests: HashMap<Address, Option<String>>,
    pub friends: HashSet<Address>,

    pub unread_messages: HashMap<Address, usize>,

    friend_event_callback: Box<dyn Fn(&friendship_event_response::Body) + Send + Sync + 'static>,
    chat_event_callback: Box<dyn Fn(DirectChatMessage) + Send + Sync + 'static>,
}

impl SocialClientHandler {
    pub fn connect(
        wallet: wallet::Wallet,
        friend_callback: impl Fn(&friendship_event_response::Body) + Send + Sync + 'static,
        chat_callback: impl Fn(DirectChatMessage) + Send + Sync + 'static,
    ) -> Option<Self> {
        let (event_sx, event_rx) = mpsc::unbounded_channel();
        let (response_sx, response_rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || social_socket_handler(wallet, event_rx, response_sx));

        Some(Self {
            is_initialized: false,
            sender: event_sx,
            friendship_receiver: response_rx,
            sent_requests: Default::default(),
            received_requests: Default::default(),
            friends: Default::default(),
            unread_messages: Default::default(),
            friend_event_callback: Box::new(friend_callback),
            chat_event_callback: Box::new(chat_callback),
        })
    }

    pub fn live(&self) -> bool {
        !self.friendship_receiver.is_closed()
    }

    pub fn friend_request(
        &mut self,
        address: Address,
        message: Option<String>,
    ) -> Result<(), anyhow::Error> {
        self.sender.send(FriendshipOutbound::FriendshipEvent(
            FriendshipEventPayload {
                body: Some(friendship_event_payload::Body::Request(RequestPayload {
                    user: Some(User {
                        address: format!("{address:#x}"),
                    }),
                    message,
                })),
            },
        ))?;
        self.sent_requests.insert(address);
        Ok(())
    }

    pub fn cancel_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        self.sender.send(FriendshipOutbound::FriendshipEvent(
            FriendshipEventPayload {
                body: Some(friendship_event_payload::Body::Cancel(CancelPayload {
                    user: Some(User {
                        address: format!("{address:#x}"),
                    }),
                })),
            },
        ))?;
        self.sent_requests.remove(&address);
        Ok(())
    }

    pub fn accept_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if self.received_requests.remove(&address).is_none() {
            return Err(anyhow!("no request"));
        };

        self.sender.send(FriendshipOutbound::FriendshipEvent(
            FriendshipEventPayload {
                body: Some(friendship_event_payload::Body::Accept(AcceptPayload {
                    user: Some(User {
                        address: format!("{address:#x}"),
                    }),
                })),
            },
        ))?;
        self.friends.insert(address);
        Ok(())
    }

    pub fn reject_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if self.received_requests.remove(&address).is_none() {
            return Err(anyhow!("no request"));
        };

        self.sender.send(FriendshipOutbound::FriendshipEvent(
            FriendshipEventPayload {
                body: Some(friendship_event_payload::Body::Reject(RejectPayload {
                    user: Some(User {
                        address: format!("{address:#x}"),
                    }),
                })),
            },
        ))?;
        Ok(())
    }

    pub fn delete_friend(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if !self.friends.remove(&address) {
            return Err(anyhow!("no request"));
        };

        self.sender.send(FriendshipOutbound::FriendshipEvent(
            FriendshipEventPayload {
                body: Some(friendship_event_payload::Body::Delete(DeletePayload {
                    user: Some(User {
                        address: format!("{address:#x}"),
                    }),
                })),
            },
        ))?;
        Ok(())
    }

    pub fn chat(&self, address: Address, message: String) -> Result<(), anyhow::Error> {
        self.sender
            .send(FriendshipOutbound::ChatMessage(DirectChatMessage {
                partner: address,
                me_speaking: true,
                message,
            }))
            .map_err(dbgerr)
    }

    pub fn get_chat_history(
        &self,
        address: Address,
    ) -> Result<Receiver<DirectChatMessage>, anyhow::Error> {
        let (sx, rx) = channel(1);
        self.sender
            .send(FriendshipOutbound::HistoryRequest(address, sx))?;
        Ok(rx)
    }

    pub fn mark_as_read(&mut self, address: Address) {
        self.unread_messages.remove(&address);
    }

    pub fn unread_messages(&self) -> &HashMap<Address, usize> {
        &self.unread_messages
    }

    pub fn update(&mut self) {
        while let Ok(rec) = self.friendship_receiver.try_recv() {
            match rec {
                FriendData::Init {
                    sent_requests,
                    received_requests,
                    friends,
                } => {
                    self.received_requests = received_requests;
                    self.sent_requests = sent_requests;
                    self.friends = friends;
                    self.is_initialized = true;
                }
                FriendData::Event(ev) => {
                    (self.friend_event_callback)(&ev);
                    match ev {
                        friendship_event_response::Body::Request(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend request (no address): {body:?}");
                                continue;
                            };
                            self.received_requests.insert(address, body.message);
                        }
                        friendship_event_response::Body::Accept(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend accept (no address): {body:?}");
                                continue;
                            };
                            self.sent_requests.remove(&address);
                            self.friends.insert(address);
                        }
                        friendship_event_response::Body::Reject(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend reject (no address): {body:?}");
                                continue;
                            };
                            self.sent_requests.remove(&address);
                        }
                        friendship_event_response::Body::Delete(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend delete (no address): {body:?}");
                                continue;
                            };
                            self.friends.remove(&address);
                        }
                        friendship_event_response::Body::Cancel(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend accept (no address): {body:?}");
                                continue;
                            };
                            self.received_requests.remove(&address);
                        }
                    }
                }
                FriendData::Chat(chat) => {
                    if !chat.me_speaking {
                        *self.unread_messages.entry(chat.partner).or_default() += 1;
                    }
                    (self.chat_event_callback)(chat);
                }
            }
        }
    }
}

fn social_socket_handler(
    wallet: wallet::Wallet,
    event_rx: UnboundedReceiver<FriendshipOutbound>,
    response_sx: UnboundedSender<FriendData>,
) {
    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    if let Err(e) = rt.block_on(social_socket_handler_inner(
        wallet.clone(),
        event_rx,
        response_sx,
    )) {
        warn!("social socket handler print: {e}");
    } else {
        debug!("k");
    }
}

fn dbgerr<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow!(format!("{e:?}"))
}

#[cfg(test)]
const MATRIX_URL: &str = "https://social.decentraland.org"; // zone doesn't work
#[cfg(not(test))]
const MATRIX_URL: &str = "https://social.decentraland.org";

#[cfg(test)]
const SOCIAL_URL: &str = "wss://rpc-social-service.decentraland.org"; // zone doesn't work
#[cfg(not(test))]
const SOCIAL_URL: &str = "wss://rpc-social-service.decentraland.org";

#[cfg(all(not(target_arch = "wasm32"), feature = "social"))]
async fn social_socket_handler_inner(
    wallet: wallet::Wallet,
    mut rx: UnboundedReceiver<FriendshipOutbound>,
    response_sx: UnboundedSender<FriendData>,
) -> Result<(), anyhow::Error> {
    let req = SocialLogin::try_new(&wallet).await?;
    let req = serde_json::to_value(&req)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    let matrix_client = matrix_sdk::Client::builder()
        .homeserver_url(MATRIX_URL)
        .build()
        .await?;
    let login = matrix_client
        .matrix_auth()
        .login_custom("m.login.decentraland", req)
        .unwrap()
        .send()
        .await?;
    let synapse_token = login.access_token;

    // create connection
    let service_connection =
        dcl_rpc::transports::web_sockets::tungstenite::WebSocketClient::connect(SOCIAL_URL)
            .await
            .map_err(dbgerr)?;
    let service_transport = WebSocketTransport::new(service_connection);
    let mut service_client = RpcClient::new(service_transport).await.unwrap();
    let port = service_client.create_port("whatever").await.unwrap();
    let service_module = port
        .load_module::<FriendshipsServiceClient<_>>("FriendshipsService")
        .await
        .map_err(dbgerr)?;

    // gather and send initial data
    let mut friends_req = service_module
        .get_friends(Payload {
            synapse_token: Some(synapse_token.clone()),
        })
        .await
        .map_err(dbgerr)?;
    let requests_req = service_module
        .get_request_events(Payload {
            synapse_token: Some(synapse_token.clone()),
        })
        .await
        .map_err(dbgerr)?;

    let mut friends = HashSet::default();
    while let Some(f) = friends_req.next().await {
        if let Some(users_response::Response::Users(Users { users })) = f.response {
            for user in users {
                if let Some(address) = user.address.as_h160() {
                    friends.insert(address);
                }
            }
        }
    }

    let mut received_requests = HashMap::default();
    let mut sent_requests = HashSet::default();
    if let Some(request_events_response::Response::Events(RequestEvents { incoming, outgoing })) =
        requests_req.response
    {
        if let Some(incoming) = incoming {
            for RequestResponse { user, message, .. } in incoming.items {
                if let Some(address) = user.and_then(|u| u.address.as_h160()) {
                    received_requests.insert(address, message);
                }
            }
        }
        if let Some(outgoing) = outgoing {
            for RequestResponse { user, .. } in outgoing.items {
                if let Some(address) = user.and_then(|u| u.address.as_h160()) {
                    sent_requests.insert(address);
                }
            }
        }
    }

    response_sx.send(FriendData::Init {
        sent_requests,
        received_requests,
        friends,
    })?;

    // build workers
    let mut inbound_service_events = service_module
        .subscribe_friendship_events_updates(Payload {
            synapse_token: Some(synapse_token.clone()),
        })
        .await
        .map_err(dbgerr)?;

    // demux the received data
    let (sx_friend, mut rx_friend) = mpsc::channel(10);
    let (sx_chat, mut rx_chat) = mpsc::channel(10);
    let (sx_history, mut rx_history) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                FriendshipOutbound::FriendshipEvent(data) => {
                    let _ = sx_friend.send(data).await;
                }
                FriendshipOutbound::ChatMessage(chat) => {
                    let _ = sx_chat.send(chat).await;
                }
                FriendshipOutbound::HistoryRequest(address, sender) => {
                    let _ = sx_history.send((address, sender)).await;
                }
            }
        }
    });

    struct RoomAliasConverter(String);
    impl RoomAliasConverter {
        fn as_ref(&self) -> Result<&RoomOrAliasId, anyhow::Error> {
            self.0.as_str().try_into().map_err(dbgerr)
        }
    }

    let room_alias = |other: Address| -> Result<RoomAliasConverter, anyhow::Error> {
        let me = format!(
            "{:#x}",
            wallet.address().ok_or(anyhow!("wallet disconnected!"))?
        );
        let other = format!("{other:#x}");
        let alias = format!(
            "#{}+{}:{}",
            (&me).min(&other),
            (&me).max(&other),
            "decentraland.org"
        )
        .to_ascii_lowercase();
        Ok(RoomAliasConverter(alias))
    };

    // outbound matrix events
    let client = matrix_client.clone();
    let f_matrix_write = async move {
        while let Some(chat) = rx_chat.recv().await {
            let alias = room_alias(chat.partner)?;
            match client.join_room_by_id_or_alias(alias.as_ref()?, &[]).await {
                Err(e) => {
                    warn!("failed to find room for address {:#?}", chat.partner);
                    warn!("err: {e}");
                    continue;
                }
                Ok(room) => {
                    room.send(RoomMessageEventContent::text_plain(chat.message))
                        .await?
                }
            };
        }

        Ok(())
    }
    .fuse();

    async fn handle_history(
        address: Address,
        alias: RoomAliasConverter,
        client: matrix_sdk::Client,
        sx: Sender<DirectChatMessage>,
    ) -> Result<(), anyhow::Error> {
        warn!("history requested for {address:#?}");
        let room = client
            .join_room_by_id_or_alias(alias.as_ref()?, &[])
            .await?;
        let mut token = None;
        let mut filter = RoomEventFilter::default();
        filter.types = Some(vec!["m.room.message".to_owned()]);

        loop {
            let mut options = MessagesOptions::backward();
            options.limit = 10u32.into();
            options.filter = filter.clone();
            options.from = token.take();

            let history = room.messages(options).await?;
            debug!("got -> {:?}", (&history.start, &history.end));
            for event in history.chunk {
                if let Ok(AnySyncTimelineEvent::MessageLike(m)) = event.raw().deserialize() {
                    if m.event_type() == MessageLikeEventType::RoomMessage {
                        let Some(sender) = matrix_to_h160(m.sender()) else {
                            warn!("no h160 from {:?}", m.sender());
                            continue;
                        };
                        let Some(AnyMessageLikeEventContent::RoomMessage(content)) =
                            m.original_content()
                        else {
                            continue;
                        };
                        let MessageType::Text(text_content) = content.msgtype else {
                            continue;
                        };
                        sx.send(DirectChatMessage {
                            partner: address,
                            me_speaking: address != sender,
                            message: text_content.body,
                        })
                        .await?;
                    }
                }
            }
            debug!("next -> {:?}", &history.end);
            token = history.end;
            if token.is_none() {
                return Ok(());
            }
        }
    }

    // history requests
    let client = matrix_client.clone();
    let f_matrix_history = async move {
        while let Some((address, sx)) = rx_history.recv().await {
            let Ok(alias) = room_alias(address) else {
                warn!("failed to get room alias");
                continue;
            };
            let client = client.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_history(address, alias, client, sx).await {
                    warn!("history err: {e}");
                }
            });
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // outbound service events
    let f_service_write = async move {
        while let Some(req) = rx_friend.recv().await {
            service_module
                .update_friendship_event(UpdateFriendshipPayload {
                    event: Some(req),
                    auth_token: Some(Payload {
                        synapse_token: Some(synapse_token.clone()),
                    }),
                })
                .await
                .map_err(dbgerr)?;
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // inbound service events
    let sx = response_sx.clone();
    let f_service_read = async move {
        while let Some(SubscribeFriendshipEventsUpdatesResponse {
            response: Some(response),
        }) = inbound_service_events.next().await
        {
            match response {
                subscribe_friendship_events_updates_response::Response::Events(evs) => {
                    for ev in evs.responses.into_iter().flat_map(|r| r.body) {
                        sx.send(FriendData::Event(ev)).map_err(dbgerr)?;
                    }
                }
                other => return Err(dbgerr(other)),
            }
        }

        Ok(())
    }
    .fuse();

    fn matrix_to_h160(s: &UserId) -> Option<Address> {
        let base = s.as_str().get(1..)?;
        base.split_once(':')
            .map(|(init, _)| init)
            .unwrap_or(base)
            .as_h160()
    }

    #[derive(Clone)]
    pub struct IsStartup(bool);

    // fn as async closures are unstable
    async fn handle_message(
        event: OriginalSyncRoomMessageEvent,
        room: Room,
        response_sx: Ctx<UnboundedSender<FriendData>>,
        is_startup: Ctx<IsStartup>,
        client: matrix_sdk::Client,
    ) {
        debug!("inbound process {event:?}");
        let Some(sender) = matrix_to_h160(&event.sender) else {
            debug!("skip 1");
            return;
        };
        let MessageType::Text(text_content) = event.content.msgtype else {
            debug!("skip 3");
            return;
        };
        let Some(user) = client.user_id() else {
            debug!("skip 4");
            return;
        };

        let Ok(members) = room.members(RoomMemberships::all()).await else {
            warn!("failed to fetch members");
            return;
        };
        let Some(partner) = members
            .iter()
            .filter(|member| !member.is_account_user())
            .flat_map(|member| matrix_to_h160(member.user_id()))
            .next()
        else {
            warn!("failed to determine partner");
            return;
        };

        if (*is_startup).0 {
            // skip if last read event is this event (we only read 1 so this should be fine, otherwise we'd need to fetch the receipt event as well to compare age)
            let read_receipt_event_id = room
                .load_user_receipt(
                    matrix_sdk::ruma::events::receipt::ReceiptType::Read,
                    ReceiptThread::Unthreaded,
                    user,
                )
                .await
                .unwrap_or(None);
            if read_receipt_event_id.is_some_and(|receipt| receipt.0 == event.event_id) {
                debug!("skip on read");
                return;
            }
        }

        let _ = response_sx.send(FriendData::Chat(DirectChatMessage {
            partner,
            me_speaking: sender != partner,
            message: text_content.body,
        }));
        if let Err(e) = room
            .send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event.event_id)
            .await
        {
            debug!("receipt err: {e:?}");
        };
        debug!("processed");
    }

    // inbound matrix events
    matrix_client.add_event_handler(handle_message);
    matrix_client.add_event_handler_context(response_sx.clone());
    matrix_client.add_event_handler_context(IsStartup(true));

    let f_matrix_read = async move {
        // limit initial history to 1 message so we can check for unread
        let mut filter = FilterDefinition::default();
        filter.room.timeline.types = Some(vec!["m.room.message".to_owned()]);
        filter.room.timeline.limit = Some(1u32.into());
        let settings = SyncSettings::default().filter(filter.into());
        matrix_client.sync(settings).await.map_err(dbgerr)?;
        matrix_client.add_event_handler_context(IsStartup(false));
        loop {
            matrix_client
                .sync(SyncSettings::default())
                .await
                .map_err(dbgerr)?;
        }
    }
    .fuse();

    // until a stream is broken
    pin_mut!(
        f_service_read,
        f_matrix_read,
        f_service_write,
        f_matrix_write,
        f_matrix_history,
    );
    select! {
        r = f_service_read => r,
        r = f_matrix_read => r,
        r = f_service_write => r,
        r = f_matrix_write => r,
        r = f_matrix_history => r,
    }
}

#[cfg(test)]
mod test {
    use std::{thread, time::Duration};

    use bevy::tasks::{IoTaskPool, TaskPoolBuilder};
    use dcl_component::proto_components::social::{
        friendship_event_response::Body, AcceptResponse, DeleteResponse, RequestResponse,
    };
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
    use wallet::Wallet;

    use crate::client::DirectChatMessage;

    use super::SocialClientHandler;

    fn blocking_recv_timeout<T>(
        client: &mut SocialClientHandler,
        r: &mut UnboundedReceiver<T>,
    ) -> Option<T> {
        for _ in 0..10 {
            client.update();
            if let Ok(data) = r.try_recv() {
                return Some(data);
            }
            thread::sleep(Duration::from_secs(1));
        }

        None
    }

    #[test]
    fn social_test() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(4).build());

        let mut wallet_a = Wallet::default();
        wallet_a.finalize_as_guest();
        let mut wallet_b = Wallet::default();
        wallet_b.finalize_as_guest();

        let (chat_a_sx, mut chat_a) = unbounded_channel();
        let (friend_a_sx, mut friend_a) = unbounded_channel();

        let (chat_b_sx, mut chat_b) = unbounded_channel();
        let (friend_b_sx, mut friend_b) = unbounded_channel();

        let mut client_a = SocialClientHandler::connect(
            wallet_a.clone(),
            move |ev| {
                friend_a_sx.send(ev.clone()).unwrap();
            },
            move |chat| {
                chat_a_sx.send(chat).unwrap();
            },
        )
        .unwrap();
        let mut client_b = SocialClientHandler::connect(
            wallet_b.clone(),
            move |ev| {
                friend_b_sx.send(ev.clone()).unwrap();
            },
            move |chat| {
                chat_b_sx.send(chat).unwrap();
            },
        )
        .unwrap();

        let mut i = 0;
        while (!client_a.is_initialized || !client_b.is_initialized) && i < 10 {
            println!("waiting for connection...");
            thread::sleep(Duration::from_secs(1));
            client_a.update();
            client_b.update();
            i += 1;
        }

        if !client_a.is_initialized || !client_b.is_initialized {
            // we can't connect, the server is probably down.
            // unfortunately this happens often enough that we can't fail CI for it, so we pass here if the server is down.
            return;
        }

        assert!(client_a.is_initialized);
        assert!(client_b.is_initialized);

        client_a
            .friend_request(wallet_b.address().unwrap(), None)
            .unwrap();
        println!("waiting for request...");
        let Some(Body::Request(RequestResponse {
            user: Some(user), ..
        })) = blocking_recv_timeout(&mut client_b, &mut friend_b)
        else {
            panic!()
        };
        assert_eq!(user.address, format!("{:#x}", wallet_a.address().unwrap()));

        client_b
            .accept_request(wallet_a.address().unwrap())
            .unwrap();
        println!("waiting for accept...");
        let Some(Body::Accept(AcceptResponse {
            user: Some(user), ..
        })) = blocking_recv_timeout(&mut client_a, &mut friend_a)
        else {
            panic!()
        };
        assert_eq!(user.address, format!("{:#x}", wallet_b.address().unwrap()));

        client_a
            .chat(wallet_b.address().unwrap(), "Hi".to_owned())
            .unwrap();
        println!("waiting for chat a->b");
        let Some(chat) = blocking_recv_timeout(&mut client_a, &mut chat_a) else {
            panic!()
        };
        assert_eq!(
            chat,
            DirectChatMessage {
                partner: wallet_b.address().unwrap(),
                me_speaking: true,
                message: "Hi".to_owned()
            }
        );
        let Some(chat) = blocking_recv_timeout(&mut client_b, &mut chat_b) else {
            panic!()
        };
        assert_eq!(
            chat,
            DirectChatMessage {
                partner: wallet_a.address().unwrap(),
                me_speaking: false,
                message: "Hi".to_owned()
            }
        );

        client_b
            .chat(wallet_a.address().unwrap(), "Hello!".to_owned())
            .unwrap();
        println!("waiting for chat b->a");
        let Some(chat) = blocking_recv_timeout(&mut client_a, &mut chat_a) else {
            panic!()
        };
        assert_eq!(
            chat,
            DirectChatMessage {
                partner: wallet_b.address().unwrap(),
                me_speaking: false,
                message: "Hello!".to_owned()
            }
        );
        let Some(chat) = blocking_recv_timeout(&mut client_b, &mut chat_b) else {
            panic!()
        };
        assert_eq!(
            chat,
            DirectChatMessage {
                partner: wallet_a.address().unwrap(),
                me_speaking: true,
                message: "Hello!".to_owned()
            }
        );

        client_a.delete_friend(wallet_b.address().unwrap()).unwrap();
        println!("waiting for delete");
        let Some(Body::Delete(DeleteResponse {
            user: Some(user), ..
        })) = blocking_recv_timeout(&mut client_b, &mut friend_b)
        else {
            panic!()
        };
        assert_eq!(user.address, format!("{:#x}", wallet_a.address().unwrap()));

        println!("done");
    }
}
