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
        _friend_callback: impl Fn(&()) + Send + Sync + 'static,
        _chat_callback: impl Fn(DirectChatMessage) + Send + Sync + 'static,
    ) -> Option<Self> {
        Some(Self::default())
    }

    pub fn update(&self) -> () {}
}
