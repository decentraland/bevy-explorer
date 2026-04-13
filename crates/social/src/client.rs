use anyhow::anyhow;
use bevy::{
    log::{debug, error, info, warn},
    platform::collections::HashMap,
};
use common::util::AsH160;
use dcl_component::proto_components::social_service::v2::{
    friendship_update, paginated_friendship_requests_response,
    upsert_friendship_payload::{
        self, AcceptPayload, CancelPayload, DeletePayload, RejectPayload, RequestPayload,
    },
    BlockUserPayload, ConnectivityStatus, FriendProfile, FriendshipRequestResponse,
    GetBlockedUsersPayload, GetFriendsPayload, GetFriendshipRequestsPayload,
    GetMutualFriendsPayload, Pagination, SocialServiceClient, SocialServiceClientDefinition,
    UnblockUserPayload, UpsertFriendshipPayload, User,
};
use dcl_rpc::{
    client::RpcClient,
    transports::web_sockets::{Message, WebSocket, WebSocketTransport},
};
use ethers_core::types::Address;
use futures_util::{pin_mut, select, FutureExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::rpc_websocket::PlatformRpcWebSocket;
use crate::runtime::SocialRuntime;
use crate::DirectChatMessage;

pub enum SocialQuery {
    GetMutualFriends {
        address: String,
        response: tokio::sync::oneshot::Sender<Result<Vec<FriendProfile>, String>>,
    },
    BlockUser {
        address: String,
        response: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    UnblockUser {
        address: String,
        response: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    GetBlockedUsers {
        response: tokio::sync::oneshot::Sender<Result<Vec<FriendProfile>, String>>,
    },
}

enum FriendData {
    Init {
        sent_requests: HashMap<Address, FriendshipRequestResponse>,
        received_requests: HashMap<Address, FriendshipRequestResponse>,
        friends: HashMap<Address, FriendProfile>,
    },
    FriendshipEvent(friendship_update::Update),
    ConnectivityEvent {
        address: Address,
        status: ConnectivityStatus,
    },
}

pub struct SocialClientHandler {
    sender: UnboundedSender<UpsertFriendshipPayload>,
    query_sender: UnboundedSender<SocialQuery>,
    friendship_receiver: UnboundedReceiver<FriendData>,

    pub is_initialized: bool,
    pub sent_requests: HashMap<Address, FriendshipRequestResponse>,
    pub received_requests: HashMap<Address, FriendshipRequestResponse>,
    pub friends: HashMap<Address, FriendProfile>,
    pub friend_status: HashMap<Address, ConnectivityStatus>,

    pub unread_messages: HashMap<Address, usize>,

    friend_event_callback: Box<dyn Fn(&friendship_update::Update) + Send + Sync + 'static>,
    connectivity_callback: Box<dyn Fn(Address, ConnectivityStatus) + Send + Sync + 'static>,
    #[allow(dead_code)]
    chat_event_callback: Box<dyn Fn(DirectChatMessage) + Send + Sync + 'static>,
}

impl SocialClientHandler {
    pub fn connect(
        wallet: wallet::Wallet,
        runtime: &SocialRuntime,
        friend_callback: impl Fn(&friendship_update::Update) + Send + Sync + 'static,
        connectivity_callback: impl Fn(Address, ConnectivityStatus) + Send + Sync + 'static,
        chat_callback: impl Fn(DirectChatMessage) + Send + Sync + 'static,
    ) -> Option<Self> {
        let (event_sx, event_rx) = mpsc::unbounded_channel();
        let (response_sx, response_rx) = mpsc::unbounded_channel();
        let (query_sx, query_rx) = mpsc::unbounded_channel();

        runtime.spawn(async move {
            if let Err(e) =
                social_socket_handler_inner(wallet, event_rx, query_rx, response_sx).await
            {
                error!("[social] socket handler error: {e}");
            } else {
                debug!("[social] socket handler finished");
            }
        });

        Some(Self {
            is_initialized: false,
            sender: event_sx,
            query_sender: query_sx,
            friendship_receiver: response_rx,
            sent_requests: Default::default(),
            received_requests: Default::default(),
            friends: Default::default(),
            friend_status: Default::default(),
            unread_messages: Default::default(),
            friend_event_callback: Box::new(friend_callback),
            connectivity_callback: Box::new(connectivity_callback),
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
        self.friend_status.remove(&address);
        Ok(())
    }

    pub fn get_mutual_friends(
        &self,
        address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<Vec<FriendProfile>, String>>, anyhow::Error>
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.query_sender.send(SocialQuery::GetMutualFriends {
            address,
            response: tx,
        })?;
        Ok(rx)
    }

    pub fn block_user(
        &self,
        address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.query_sender.send(SocialQuery::BlockUser {
            address,
            response: tx,
        })?;
        Ok(rx)
    }

    pub fn unblock_user(
        &self,
        address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.query_sender.send(SocialQuery::UnblockUser {
            address,
            response: tx,
        })?;
        Ok(rx)
    }

    pub fn get_blocked_users(
        &self,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<Vec<FriendProfile>, String>>, anyhow::Error>
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.query_sender
            .send(SocialQuery::GetBlockedUsers { response: tx })?;
        Ok(rx)
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
                FriendData::FriendshipEvent(ev) => {
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
                            self.received_requests.insert(
                                address,
                                FriendshipRequestResponse {
                                    friend: body.friend.clone(),
                                    created_at: body.created_at,
                                    message: body.message.clone(),
                                    id: body.id.clone(),
                                },
                            );
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
                            self.friend_status.remove(&address);
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
                        friendship_update::Update::Block(body) => {
                            let Some(address) =
                                body.user.as_ref().and_then(|u| u.address.as_h160())
                            else {
                                warn!("invalid block event (no address): {body:?}");
                                continue;
                            };
                            // When someone blocks us, remove them from friends and requests
                            self.friends.remove(&address);
                            self.friend_status.remove(&address);
                            self.sent_requests.remove(&address);
                            self.received_requests.remove(&address);
                        }
                    }
                }
                FriendData::ConnectivityEvent { address, status } => {
                    if self.friends.contains_key(&address) {
                        self.friend_status.insert(address, status);
                        (self.connectivity_callback)(address, status);
                    }
                }
            }
        }
    }
}

fn dbgerr<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow!(format!("{e:?}"))
}

const SOCIAL_URL: &str = "wss://rpc-social-service-ea.decentraland.org";

const PAGE_SIZE: i32 = 100;

async fn social_socket_handler_inner(
    wallet: wallet::Wallet,
    mut rx: UnboundedReceiver<UpsertFriendshipPayload>,
    mut query_rx: UnboundedReceiver<SocialQuery>,
    response_sx: UnboundedSender<FriendData>,
) -> Result<(), anyhow::Error> {
    // Connect WebSocket
    info!("[social] Connecting to social service at {SOCIAL_URL}");
    let ws = PlatformRpcWebSocket::connect(SOCIAL_URL)
        .await
        .map_err(dbgerr)?;
    info!("[social] Successfully connected to social service at {SOCIAL_URL}");

    // V2 auth: send signed headers as first WS message
    let uri: http::Uri = SOCIAL_URL.parse().map_err(dbgerr)?;
    let signed_headers = wallet::sign_request("get", &uri, &wallet, "{}".to_owned())
        .await
        .map_err(dbgerr)?;
    let headers_map: std::collections::HashMap<String, String> =
        signed_headers.into_iter().collect();
    let auth_json = serde_json::to_string(&headers_map)?;
    info!("[social] Sending auth headers: {auth_json}");
    ws.send(Message::Text(auth_json)).await.map_err(dbgerr)?;

    // Create RPC client
    let service_transport = WebSocketTransport::new(ws);
    let mut service_client = RpcClient::new(service_transport)
        .await
        .map_err(|e| anyhow!("[social] Failed to create RPC client: {e:?}"))?;
    let port = service_client
        .create_port("social")
        .await
        .map_err(|e| anyhow!("[social] Failed to create port: {e:?}"))?;
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

        info!(
            "[social] get_friends(offset={offset}): got {} friends, raw response: {:?}",
            resp.friends.len(),
            resp.pagination_data
        );

        for friend in resp.friends {
            if let Some(address) = friend.address.as_h160() {
                friends.insert(address, friend);
            }
        }

        let total = resp.pagination_data.as_ref().map(|p| p.total).unwrap_or(0);
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
    info!(
        "[social] get_pending_friendship_requests response: {:?}",
        pending_resp.response
    );

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
    info!(
        "[social] Pending requests loaded: {}",
        received_requests.len()
    );

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
    info!(
        "[social] get_sent_friendship_requests response: {:?}",
        sent_resp.response
    );

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

    info!(
        "[social] Init complete — friends: {}, received_requests: {}, sent_requests: {}",
        friends.len(),
        received_requests.len(),
        sent_requests.len()
    );
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

    // Subscribe to friend connectivity updates
    info!("[social] Subscribing to friend connectivity updates...");
    let mut connectivity_updates = service_module
        .subscribe_to_friend_connectivity_updates()
        .await
        .map_err(dbgerr)?;
    info!("[social] Subscribed to friend connectivity updates");

    // Outbound: send friendship actions + handle queries
    let f_service_write = async move {
        loop {
            tokio::select! {
                req = rx.recv() => {
                    let Some(req) = req else { break; };
                    info!("[social] upsert_friendship request: {req:?}");
                    let resp = service_module
                        .upsert_friendship(req)
                        .await
                        .map_err(|e| anyhow!("[social] upsert_friendship transport error: {e:?}"))?;
                    info!("[social] upsert_friendship response: {resp:?}");
                }
                query = query_rx.recv() => {
                    let Some(query) = query else { break; };
                    match query {
                        SocialQuery::GetMutualFriends { address, response } => {
                            info!("[social] getMutualFriends request for {address}");
                            let mut all_friends = Vec::new();
                            let mut offset = 0;
                            let mut result: Result<Vec<FriendProfile>, String> = Ok(Vec::new());
                            loop {
                                match service_module.get_mutual_friends(GetMutualFriendsPayload {
                                    user: Some(User { address: address.clone() }),
                                    pagination: Some(Pagination { limit: PAGE_SIZE, offset }),
                                }).await {
                                    Ok(resp) => {
                                        let count = resp.friends.len();
                                        all_friends.extend(resp.friends);
                                        let total = resp.pagination_data.as_ref().map(|p| p.total).unwrap_or(0);
                                        offset += PAGE_SIZE;
                                        if offset >= total || count == 0 { break; }
                                    }
                                    Err(e) => {
                                        warn!("[social] getMutualFriends error: {e:?}");
                                        result = Err(format!("{e:?}"));
                                        break;
                                    }
                                }
                            }
                            if result.is_ok() {
                                info!("[social] getMutualFriends: {} mutual friends", all_friends.len());
                                let _ = response.send(Ok(all_friends));
                            } else {
                                let _ = response.send(result);
                            }
                        }
                        SocialQuery::BlockUser { address, response } => {
                            info!("[social] blockUser request for {address}");
                            match service_module.block_user(BlockUserPayload {
                                user: Some(User { address: address.clone() }),
                            }).await {
                                Ok(resp) => {
                                    use dcl_component::proto_components::social_service::v2::block_user_response::Response;
                                    match resp.response {
                                        Some(Response::Ok(_)) => {
                                            info!("[social] blockUser success for {address}");
                                            let _ = response.send(Ok(()));
                                        }
                                        Some(Response::InternalServerError(e)) => {
                                            let msg = e.message.unwrap_or_default();
                                            warn!("[social] blockUser internal error: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        Some(Response::InvalidRequest(e)) => {
                                            let msg = e.message.unwrap_or_default();
                                            warn!("[social] blockUser invalid request: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        Some(Response::ProfileNotFound(e)) => {
                                            let msg = e.message.unwrap_or_else(|| "profile not found".to_string());
                                            warn!("[social] blockUser profile not found: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        None => {
                                            let _ = response.send(Err("empty response".to_string()));
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("[social] blockUser error: {e:?}");
                                    let _ = response.send(Err(format!("{e:?}")));
                                }
                            }
                        }
                        SocialQuery::UnblockUser { address, response } => {
                            info!("[social] unblockUser request for {address}");
                            match service_module.unblock_user(UnblockUserPayload {
                                user: Some(User { address: address.clone() }),
                            }).await {
                                Ok(resp) => {
                                    use dcl_component::proto_components::social_service::v2::unblock_user_response::Response;
                                    match resp.response {
                                        Some(Response::Ok(_)) => {
                                            info!("[social] unblockUser success for {address}");
                                            let _ = response.send(Ok(()));
                                        }
                                        Some(Response::InternalServerError(e)) => {
                                            let msg = e.message.unwrap_or_default();
                                            warn!("[social] unblockUser internal error: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        Some(Response::InvalidRequest(e)) => {
                                            let msg = e.message.unwrap_or_default();
                                            warn!("[social] unblockUser invalid request: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        Some(Response::ProfileNotFound(e)) => {
                                            let msg = e.message.unwrap_or_else(|| "profile not found".to_string());
                                            warn!("[social] unblockUser profile not found: {msg}");
                                            let _ = response.send(Err(msg));
                                        }
                                        None => {
                                            let _ = response.send(Err("empty response".to_string()));
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("[social] unblockUser error: {e:?}");
                                    let _ = response.send(Err(format!("{e:?}")));
                                }
                            }
                        }
                        SocialQuery::GetBlockedUsers { response } => {
                            info!("[social] getBlockedUsers request");
                            let mut all_profiles = Vec::new();
                            let mut offset = 0;
                            let mut result: Result<Vec<FriendProfile>, String> = Ok(Vec::new());
                            loop {
                                match service_module.get_blocked_users(GetBlockedUsersPayload {
                                    pagination: Some(Pagination { limit: PAGE_SIZE, offset }),
                                }).await {
                                    Ok(resp) => {
                                        let count = resp.profiles.len();
                                        // Convert BlockedUserProfile to FriendProfile
                                        for blocked in &resp.profiles {
                                            all_profiles.push(FriendProfile {
                                                address: blocked.address.clone(),
                                                name: blocked.name.clone(),
                                                has_claimed_name: blocked.has_claimed_name,
                                                profile_picture_url: blocked.profile_picture_url.clone(),
                                                name_color: blocked.name_color,
                                            });
                                        }
                                        let total = resp.pagination_data.as_ref().map(|p| p.total).unwrap_or(0);
                                        offset += PAGE_SIZE;
                                        if offset >= total || count == 0 { break; }
                                    }
                                    Err(e) => {
                                        warn!("[social] getBlockedUsers error: {e:?}");
                                        result = Err(format!("{e:?}"));
                                        break;
                                    }
                                }
                            }
                            if result.is_ok() {
                                info!("[social] getBlockedUsers: {} blocked users", all_profiles.len());
                                let _ = response.send(Ok(all_profiles));
                            } else {
                                let _ = response.send(result);
                            }
                        }
                    }
                }
            }
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // Inbound: receive friendship update events
    let sx_friendship = response_sx.clone();
    let f_service_read = async move {
        while let Some(update) = inbound_updates.next().await {
            info!("[social] Received friendship update: {update:?}");
            if let Some(ev) = update.update {
                sx_friendship
                    .send(FriendData::FriendshipEvent(ev))
                    .map_err(dbgerr)?;
            }
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // Inbound: receive friend connectivity update events
    let sx_connectivity = response_sx.clone();
    let f_connectivity_read = async move {
        while let Some(update) = connectivity_updates.next().await {
            info!("[social] Received connectivity update: {update:?}");
            if let Some(friend) = &update.friend {
                if let Some(address) = friend.address.as_h160() {
                    let status = match update.status {
                        0 => ConnectivityStatus::Online,
                        2 => ConnectivityStatus::Away,
                        _ => ConnectivityStatus::Offline,
                    };
                    sx_connectivity
                        .send(FriendData::ConnectivityEvent { address, status })
                        .map_err(dbgerr)?;
                }
            }
        }
        Result::<(), anyhow::Error>::Ok(())
    }
    .fuse();

    // Run until a stream breaks
    pin_mut!(f_service_read, f_service_write, f_connectivity_read);
    select! {
        r = f_service_read => r,
        r = f_service_write => r,
        r = f_connectivity_read => r,
    }
}

pub type FriendshipEventBody = friendship_update::Update;
