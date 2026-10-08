//! Local DM history: every DM sent or received is stored per account, with an index of the
//! partners that have one (the stored files' names cannot be read back into addresses). Storage
//! is the platform crate's (encrypted files on native, IndexedDB on web). Nothing here talks to
//! a server, and which conversations the HUD shows is the HUD's business.

use alloy_core::primitives::Address;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use common::{
    rpc::RpcResultSender,
    structs::PrimaryUser,
    util::{AsH160, TaskExt},
};
use comms::{global_crdt::ChatEvent, private_chat::PrivateChatReceived};
use console::PendingConsoleResponses;
use platform::DmHistoryEntry;
use system_bridge::{DmHistoryEntryData, SystemApi};
use wallet::Wallet;

/// The partners the local account has stored DMs with, loaded once per login.
#[derive(Resource, Default)]
pub struct DmConversations {
    account: Option<Address>,
    partners: Vec<String>,
    loading: Option<Task<Result<Vec<String>, String>>>,
    loaded: bool,
}

impl DmConversations {
    fn account_hex(&self) -> Option<String> {
        self.account.map(|a| format!("{a:#x}"))
    }

    /// Adds `partner` if absent; true if the list changed.
    fn add(&mut self, partner: &str) -> bool {
        if self.partners.iter().any(|p| p == partner) {
            return false;
        }
        self.partners.push(partner.to_owned());
        true
    }

    fn remove(&mut self, partner: &str) -> bool {
        let before = self.partners.len();
        self.partners.retain(|p| p != partner);
        self.partners.len() != before
    }

    fn persist(&self) {
        let Some(account) = self.account_hex() else {
            return;
        };
        let partners = self.partners.clone();
        IoTaskPool::get()
            .spawn(async move {
                if let Err(e) = platform::dm_conversations_write(&account, &partners).await {
                    warn!("failed to store dm conversations: {e}");
                }
            })
            .detach();
    }
}

fn now_unix() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn normalize(address: &str) -> Option<String> {
    address.as_h160().map(|a| format!("{a:#x}"))
}

/// Follows the wallet: resets and reloads the index on login, drops it on logout.
pub fn load_dm_conversations(wallet: Res<Wallet>, mut conversations: ResMut<DmConversations>) {
    if wallet.is_changed() && conversations.account != wallet.address() {
        conversations.account = wallet.address();
        conversations.partners.clear();
        conversations.loading = None;
        conversations.loaded = false;
    }
    if conversations.loaded || conversations.loading.is_some() {
        if let Some(mut task) = conversations.loading.take() {
            match task.complete() {
                None => conversations.loading = Some(task),
                Some(Ok(partners)) => {
                    conversations.partners = partners;
                    conversations.loaded = true;
                }
                Some(Err(e)) => {
                    warn!("failed to load dm conversations: {e}");
                    conversations.loaded = true;
                }
            }
        }
        return;
    }
    let Some(account) = conversations.account_hex() else {
        return;
    };
    conversations.loading = Some(
        IoTaskPool::get().spawn(async move { platform::dm_conversations_read(&account).await }),
    );
}

/// Stores every DM sent or received and keeps the partner index current.
pub fn record_dms(
    mut chats: EventReader<ChatEvent>,
    mut received: EventReader<PrivateChatReceived>,
    player: Query<Entity, With<PrimaryUser>>,
    mut conversations: ResMut<DmConversations>,
) {
    let Some(account) = conversations.account_hex() else {
        chats.clear();
        received.clear();
        return;
    };
    let player = player.single().ok();

    let mut entries: Vec<(String, DmHistoryEntry)> = Vec::new();
    for ev in chats.read() {
        if Some(ev.sender) != player {
            continue;
        }
        let Some(partner) = normalize(&ev.channel) else {
            continue;
        };
        entries.push((
            partner,
            DmHistoryEntry {
                from: account.clone(),
                message: ev.message.clone(),
                timestamp: ev.timestamp,
                received_at: now_unix(),
            },
        ));
    }
    for dm in received.read() {
        let partner = format!("{:#x}", dm.from);
        entries.push((
            partner.clone(),
            DmHistoryEntry {
                from: partner,
                message: dm.message.clone(),
                timestamp: dm.timestamp,
                received_at: now_unix(),
            },
        ));
    }

    let mut list_changed = false;
    for (partner, entry) in entries {
        list_changed |= conversations.add(&partner);
        let account = account.clone();
        IoTaskPool::get()
            .spawn(async move {
                if let Err(e) = platform::dm_history_append(&account, &partner, &entry).await {
                    warn!("failed to store dm: {e}");
                }
            })
            .detach();
    }
    if list_changed {
        conversations.persist();
    }
}

pub fn handle_dm_history_requests(
    mut requests: EventReader<SystemApi>,
    mut conversations: ResMut<DmConversations>,
) {
    for request in requests.read() {
        match request {
            SystemApi::GetDmConversations(sender) => {
                sender.send(conversations.partners.clone());
            }
            SystemApi::GetDmHistory(partner, sender) => {
                let sender = sender.clone();
                let (Some(account), Some(partner)) =
                    (conversations.account_hex(), normalize(partner))
                else {
                    sender.send(Vec::new());
                    continue;
                };
                IoTaskPool::get()
                    .spawn(async move {
                        let entries = match platform::dm_history_read(&account, &partner).await {
                            Ok(entries) => entries,
                            Err(e) => {
                                warn!("failed to read dm history with {partner}: {e}");
                                Vec::new()
                            }
                        };
                        sender.send(
                            entries
                                .into_iter()
                                .map(|e| DmHistoryEntryData {
                                    from: e.from,
                                    message: e.message,
                                    timestamp: e.timestamp,
                                    received_at: e.received_at,
                                })
                                .collect(),
                        );
                    })
                    .detach();
            }
            SystemApi::DeleteDmHistory(partner) => {
                let (Some(account), Some(partner)) =
                    (conversations.account_hex(), normalize(partner))
                else {
                    continue;
                };
                if conversations.remove(&partner) {
                    conversations.persist();
                }
                IoTaskPool::get()
                    .spawn(async move {
                        if let Err(e) = platform::dm_history_delete(&account, &partner).await {
                            warn!("failed to delete dm history with {partner}: {e}");
                        }
                    })
                    .detach();
            }
            _ => (),
        }
    }
}

/// `/dm_conversations` lists the partners with stored DMs, oldest first.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_conversations")]
pub struct DmConversationsCommand;

pub fn dm_conversations(
    mut input: ConsoleCommand<DmConversationsCommand>,
    conversations: Res<DmConversations>,
) {
    if input.take().is_none() {
        return;
    }
    input.reply_ok(if conversations.partners.is_empty() {
        "no stored conversations".to_owned()
    } else {
        conversations.partners.join("\n")
    });
}

/// `/dm_history <address> [count]` prints the last `count` (default 20) stored DMs with a partner.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_history")]
pub struct DmHistoryCommand {
    address: String,
    count: Option<usize>,
}

pub fn dm_history(
    mut input: ConsoleCommand<DmHistoryCommand>,
    mut events: EventWriter<SystemApi>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    let Some(Ok(command)) = input.take() else {
        return;
    };
    let count = command.count.unwrap_or(20);
    let (sx, rx) = RpcResultSender::<Vec<DmHistoryEntryData>>::channel();
    events.write(SystemApi::GetDmHistory(command.address, sx));
    let responder = input.take_responder();
    pending.push_receiver(
        rx,
        move |entries| {
            let skipped = entries.len().saturating_sub(count);
            Ok(entries
                .iter()
                .skip(skipped)
                .map(|e| format!("[{}] {}: {}", e.timestamp, e.from, e.message))
                .collect::<Vec<_>>()
                .join("\n"))
        },
        responder,
    );
}

/// `/dm_delete <address>` deletes the stored DMs with a partner.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_delete")]
pub struct DmDeleteCommand {
    address: String,
}

pub fn dm_delete(mut input: ConsoleCommand<DmDeleteCommand>, mut events: EventWriter<SystemApi>) {
    if let Some(Ok(command)) = input.take() {
        events.write(SystemApi::DeleteDmHistory(command.address));
        input.reply_ok("");
    }
}
