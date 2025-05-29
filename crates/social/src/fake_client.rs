use bevy::utils::{HashMap, HashSet};
use ethers_core::types::Address;

use crate::DirectChatMessage;

#[derive(Default)]
pub struct SocialClientHandler {
    pub is_initialized: bool,
    pub sent_requests: HashSet<Address>,
    pub received_requests: HashMap<Address, Option<String>>,
    pub friends: HashSet<Address>,

    pub unread_messages: HashMap<Address, usize>,
}

impl SocialClientHandler {
    pub fn connect(
        _wallet: wallet::Wallet,
        _friend_callback: impl Fn(&FriendshipEventBody) + Send + Sync + 'static,
        _chat_callback: impl Fn(DirectChatMessage) + Send + Sync + 'static,
    ) -> Option<Self> {
        Some(Self::default())
    }

    pub fn update(&self) -> () {}

    
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
    Request(BodyData),
    Accept(BodyData),
    Reject(BodyData),
    Delete(BodyData),
    Cancel(BodyData),
}

#[derive(Clone, Debug)]
pub struct BodyData {
    pub user: Option<BodyDataInner>,
}

#[derive(Clone, Debug)]
pub struct BodyDataInner {
    pub address: String,
}
