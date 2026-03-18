use anyhow::anyhow;
use bevy::{
    log::{debug, error, info, warn},
    platform::collections::HashMap,
};
use common::util::AsH160;
use dcl_component::proto_components::social_service::v2::{
    friendship_update, paginated_friendship_requests_response,
    upsert_friendship_payload::{self, AcceptPayload, CancelPayload, DeletePayload, RejectPayload, RequestPayload},
    FriendProfile, FriendshipRequestResponse, GetFriendshipRequestsPayload, GetFriendsPayload,
    Pagination, SocialServiceClient, SocialServiceClientDefinition, UpsertFriendshipPayload, User,
};
use dcl_rpc::{
    client::RpcClient,
    transports::web_sockets::{Message, WebSocket, WebSocketTransport},
};
use ethers_core::types::Address;
use futures_util::{pin_mut, select, FutureExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::DirectChatMessage;

enum FriendData {
    Init {
        sent_requests: HashMap<Address, FriendshipRequestResponse>,
        received_requests: HashMap<Address, FriendshipRequestResponse>,
        friends: HashMap<Address, FriendProfile>,
    },
    Event(friendship_update::Update),
}

pub struct SocialClientHandler {
    sender: UnboundedSender<UpsertFriendshipPayload>,
    friendship_receiver: UnboundedReceiver<FriendData>,

    pub is_initialized: bool,
    pub sent_requests: HashMap<Address, FriendshipRequestResponse>,
    pub received_requests: HashMap<Address, FriendshipRequestResponse>,
    pub friends: HashMap<Address, FriendProfile>,

    pub unread_messages: HashMap<Address, usize>,

    friend_event_callback: Box<dyn Fn(&friendship_update::Update) + Send + Sync + 'static>,
    #[allow(dead_code)]
    chat_event_callback: Box<dyn Fn(DirectChatMessage) + Send + Sync + 'static>,
}

impl SocialClientHandler {
    pub fn connect(
        wallet: wallet::Wallet,
        friend_callback: impl Fn(&friendship_update::Update) + Send + Sync + 'static,
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

    fn make_user(address: Address) -> Option<User> {
        Some(User {
            address: format!("{address:#x}"),
        })
    }

    pub fn friend_request(
        &mut self,
        address: Address,
        message: Option<String>,
    ) -> Result<(), anyhow::Error> {
        self.sender.send(UpsertFriendshipPayload {
            action: Some(upsert_friendship_payload::Action::Request(RequestPayload {
                user: Self::make_user(address),
                message,
            })),
        })?;
        Ok(())
    }

    pub fn cancel_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        self.sender.send(UpsertFriendshipPayload {
            action: Some(upsert_friendship_payload::Action::Cancel(CancelPayload {
                user: Self::make_user(address),
            })),
        })?;
        self.sent_requests.remove(&address);
        Ok(())
    }

    pub fn accept_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if self.received_requests.remove(&address).is_none() {
            return Err(anyhow!("no request"));
        };

        self.sender.send(UpsertFriendshipPayload {
            action: Some(upsert_friendship_payload::Action::Accept(AcceptPayload {
                user: Self::make_user(address),
            })),
        })?;
        Ok(())
    }

    pub fn reject_request(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if self.received_requests.remove(&address).is_none() {
            return Err(anyhow!("no request"));
        };

        self.sender.send(UpsertFriendshipPayload {
            action: Some(upsert_friendship_payload::Action::Reject(RejectPayload {
                user: Self::make_user(address),
            })),
        })?;
        Ok(())
    }

    pub fn delete_friend(&mut self, address: Address) -> Result<(), anyhow::Error> {
        if self.friends.remove(&address).is_none() {
            return Err(anyhow!("no request"));
        };

        self.sender.send(UpsertFriendshipPayload {
            action: Some(upsert_friendship_payload::Action::Delete(DeletePayload {
                user: Self::make_user(address),
            })),
        })?;
        Ok(())
    }

    pub fn chat(&self, _address: Address, _message: String) -> Result<(), anyhow::Error> {
        // DM chat not supported in V2 (no Matrix)
        Err(anyhow!("chat not available in V2"))
    }

    pub fn get_chat_history(
        &self,
        _address: Address,
    ) -> Result<tokio::sync::mpsc::Receiver<DirectChatMessage>, anyhow::Error> {
        // Chat history via Matrix is no longer supported in V2
        Err(anyhow!("chat history not available in V2"))
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
                        friendship_update::Update::Request(body) => {
                            let Some(friend_profile) = body.friend.as_ref() else {
                                warn!("invalid friend request (no friend profile): {body:?}");
                                continue;
                            };
                            let Some(address) = friend_profile.address.as_h160() else {
                                warn!("invalid friend request (no address): {body:?}");
                                continue;
                            };
                            self.received_requests.insert(address, FriendshipRequestResponse {
                                friend: body.friend.clone(),
                                created_at: body.created_at,
                                message: body.message.clone(),
                                id: body.id.clone(),
                            });
                        }
                        friendship_update::Update::Accept(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend accept (no address): {body:?}");
                                continue;
                            };
                            // Move from sent_requests to friends
                            if let Some(req) = self.sent_requests.remove(&address) {
                                if let Some(profile) = req.friend {
                                    self.friends.insert(address, profile);
                                }
                            }
                        }
                        friendship_update::Update::Reject(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend reject (no address): {body:?}");
                                continue;
                            };
                            self.sent_requests.remove(&address);
                        }
                        friendship_update::Update::Delete(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend delete (no address): {body:?}");
                                continue;
                            };
                            self.friends.remove(&address);
                        }
                        friendship_update::Update::Cancel(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid friend cancel (no address): {body:?}");
                                continue;
                            };
                            self.received_requests.remove(&address);
                        }
                        friendship_update::Update::Block(_) => {
                            // TODO: handle block events
                        }
                    }
                }
            }
        }
    }
}

fn social_socket_handler(
    wallet: wallet::Wallet,
    event_rx: UnboundedReceiver<UpsertFriendshipPayload>,
    response_sx: UnboundedSender<FriendData>,
) {
    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    if let Err(e) = rt.block_on(social_socket_handler_inner(
        wallet,
        event_rx,
        response_sx,
    )) {
        error!("[social] socket handler error: {e}");
    } else {
        debug!("social socket handler finished");
    }
}

fn dbgerr<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow!(format!("{e:?}"))
}

const SOCIAL_URL: &str = "wss://rpc-social-service-ea.decentraland.org";

const PAGE_SIZE: i32 = 100;

#[cfg(all(not(target_arch = "wasm32"), feature = "social"))]
async fn social_socket_handler_inner(
    wallet: wallet::Wallet,
    mut rx: UnboundedReceiver<UpsertFriendshipPayload>,
    response_sx: UnboundedSender<FriendData>,
) -> Result<(), anyhow::Error> {
    // Connect WebSocket
    info!("[social] Connecting to social service at {SOCIAL_URL}");
    let ws =
        dcl_rpc::transports::web_sockets::tungstenite::WebSocketClient::connect(SOCIAL_URL)
            .await
            .map_err(dbgerr)?;
    info!("[social] Successfully connected to social service at {SOCIAL_URL}");

    // V2 auth: send signed headers as first WS message
    let uri: http::Uri = SOCIAL_URL.parse().map_err(dbgerr)?;
    let signed_headers = wallet::sign_request("get", &uri, &wallet, "{}".to_owned())
        .await
        .map_err(dbgerr)?;
    let headers_map: std::collections::HashMap<String, String> = signed_headers.into_iter().collect();
    let auth_json = serde_json::to_string(&headers_map)?;
    info!("[social] Sending auth headers: {auth_json}");
    ws.send(Message::Text(auth_json)).await.map_err(dbgerr)?;

    // Create RPC client
    let service_transport = WebSocketTransport::new(ws);
    let mut service_client = RpcClient::new(service_transport).await.map_err(|e| anyhow!("[social] Failed to create RPC client: {e:?}"))?;
    let port = service_client.create_port("social").await.map_err(|e| anyhow!("[social] Failed to create port: {e:?}"))?;
    let service_module = port
        .load_module::<SocialServiceClient<_>>("SocialService")
        .await
        .map_err(dbgerr)?;

    // Gather initial data: friends list (paginated)
    info!("[social] Fetching friends list...");
    let mut friends = HashMap::default();
    let mut offset = 0;
    loop {
        let resp = service_module
            .get_friends(GetFriendsPayload {
                pagination: Some(Pagination {
                    limit: PAGE_SIZE,
                    offset,
                }),
            })
            .await
            .map_err(dbgerr)?;

        info!("[social] get_friends(offset={offset}): got {} friends, raw response: {:?}", resp.friends.len(), resp.pagination_data);

        for friend in resp.friends {
            if let Some(address) = friend.address.as_h160() {
                friends.insert(address, friend);
            }
        }

        let total = resp
            .pagination_data
            .as_ref()
            .map(|p| p.total)
            .unwrap_or(0);
        offset += PAGE_SIZE;
        if offset >= total || friends.is_empty() {
            break;
        }
    }
    info!("[social] Total friends loaded: {}", friends.len());

    // Gather initial data: received (pending) requests
    info!("[social] Fetching pending friendship requests...");
    let mut received_requests = HashMap::new();
    let pending_resp = service_module
        .get_pending_friendship_requests(GetFriendshipRequestsPayload {
            pagination: Some(Pagination {
                limit: PAGE_SIZE,
                offset: 0,
            }),
        })
        .await
        .map_err(dbgerr)?;
    info!("[social] get_pending_friendship_requests response: {:?}", pending_resp.response);

    if let Some(paginated_friendship_requests_response::Response::Requests(reqs)) =
        pending_resp.response
    {
        for req in reqs.requests {
            if let Some(friend) = &req.friend {
                if let Some(address) = friend.address.as_h160() {
                    received_requests.insert(address, req);
                }
            }
        }
    }
    info!("[social] Pending requests loaded: {}", received_requests.len());

    // Gather initial data: sent requests
    info!("[social] Fetching sent friendship requests...");
    let mut sent_requests = HashMap::new();
    let sent_resp = service_module
        .get_sent_friendship_requests(GetFriendshipRequestsPayload {
            pagination: Some(Pagination {
                limit: PAGE_SIZE,
                offset: 0,
            }),
        })
        .await
        .map_err(dbgerr)?;
    info!("[social] get_sent_friendship_requests response: {:?}", sent_resp.response);

    if let Some(paginated_friendship_requests_response::Response::Requests(reqs)) =
        sent_resp.response
    {
        for req in reqs.requests {
            if let Some(friend) = &req.friend {
                if let Some(address) = friend.address.as_h160() {
                    sent_requests.insert(address, req);
                }
            }
        }
    }
    info!("[social] Sent requests loaded: {}", sent_requests.len());

    info!("[social] Init complete — friends: {}, received_requests: {}, sent_requests: {}", friends.len(), received_requests.len(), sent_requests.len());
    response_sx.send(FriendData::Init {
        sent_requests,
        received_requests,
        friends,
    })?;

    // Subscribe to friendship updates
    info!("[social] Subscribing to friendship updates...");
    let mut inbound_updates = service_module
        .subscribe_to_friendship_updates()
        .await
        .map_err(dbgerr)?;
    info!("[social] Subscribed to friendship updates");

    // Outbound: send friendship actions
    let f_service_write = async move {
        while let Some(req) = rx.recv().await {
            info!("[social] upsert_friendship request: {req:?}");
            let resp = service_module
                .upsert_friendship(req)
                .await
                .map_err(dbgerr)?;
            info!("[social] upsert_friendship response: {resp:?}");
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // Inbound: receive friendship update events
    let sx = response_sx.clone();
    let f_service_read = async move {
        while let Some(update) = inbound_updates.next().await {
            info!("[social] Received friendship update: {update:?}");
            if let Some(ev) = update.update {
                sx.send(FriendData::Event(ev)).map_err(dbgerr)?;
            }
        }
        Ok(())
    }
    .fuse();

    // Run until a stream breaks
    pin_mut!(f_service_read, f_service_write);
    select! {
        r = f_service_read => r,
        r = f_service_write => r,
    }
}

pub type FriendshipEventBody = friendship_update::Update;
