use bevy::platform::collections::HashMap;
use ethers_core::types::Address;

use crate::DirectChatMessage;

/// Stub types mirroring the proto FriendProfile / FriendshipRequestResponse
/// used when the `social` feature is disabled.
#[derive(Clone, Debug, Default)]
pub struct FriendProfile {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
}

#[derive(Clone, Debug, Default)]
pub struct FriendshipRequestResponse {
    pub friend: Option<FriendProfile>,
    pub created_at: i64,
    pub message: Option<String>,
    pub id: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectivityStatus {
    Online = 0,
    #[default]
    Offline = 1,
    Away = 2,
}

#[derive(Default)]
pub struct SocialClientHandler {
    pub is_initialized: bool,
    pub sent_requests: HashMap<Address, FriendshipRequestResponse>,
    pub received_requests: HashMap<Address, FriendshipRequestResponse>,
    pub friends: HashMap<Address, FriendProfile>,
    pub friend_status: HashMap<Address, ConnectivityStatus>,

    pub unread_messages: HashMap<Address, usize>,
}

impl SocialClientHandler {
    pub fn connect(
        _wallet: wallet::Wallet,
        _friend_callback: impl Fn(&FriendshipEventBody) + Send + Sync + 'static,
        _connectivity_callback: impl Fn(Address, ConnectivityStatus) + Send + Sync + 'static,
        _chat_callback: impl Fn(DirectChatMessage) + Send + Sync + 'static,
    ) -> Option<Self> {
        Some(Self::default())
    }

    pub fn update(&self) {}

    pub fn live(&self) -> bool {
        false
    }

    pub fn friend_request(
        &mut self,
        _address: Address,
        _message: Option<String>,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn cancel_request(&mut self, _address: Address) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn accept_request(&mut self, _address: Address) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn reject_request(&mut self, _address: Address) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn delete_friend(&mut self, _address: Address) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn get_mutual_friends(
        &self,
        _address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<Vec<FriendProfile>, String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Ok(Vec::new()));
        Ok(rx)
    }

    pub fn block_user(
        &self,
        _address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Ok(()));
        Ok(rx)
    }

    pub fn unblock_user(
        &self,
        _address: String,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Ok(()));
        Ok(rx)
    }

    pub fn get_blocked_users(
        &self,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<Vec<FriendProfile>, String>>, anyhow::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Ok(Vec::new()));
        Ok(rx)
    }

    pub fn chat(&self, _address: Address, _message: String) -> Result<(), anyhow::Error> {
        Ok(())
    }

    pub fn get_chat_history(
        &self,
        _address: Address,
    ) -> Result<tokio::sync::mpsc::Receiver<DirectChatMessage>, anyhow::Error> {
        Err(anyhow::anyhow!("not implemented"))
    }

    pub fn mark_as_read(&mut self, address: Address) {
        self.unread_messages.remove(&address);
    }

    pub fn unread_messages(&self) -> &HashMap<Address, usize> {
        &self.unread_messages
    }
}

#[derive(Clone, Debug)]
pub enum FriendshipEventBody {
    Request(RequestBodyData),
    Accept(BodyData),
    Reject(BodyData),
    Delete(BodyData),
    Cancel(BodyData),
    Block(BodyData),
}

#[derive(Clone, Debug)]
pub struct RequestBodyData {
    pub friend: Option<BodyDataInner>,
}

#[derive(Clone, Debug)]
pub struct BodyData {
    pub user: Option<BodyDataInner>,
}

#[derive(Clone, Debug)]
pub struct BodyDataInner {
    pub address: String,
}
