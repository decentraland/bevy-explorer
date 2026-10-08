//! Local DM history: every DM sent or received is stored per account and read back per partner.
//! Storage is the platform crate's (encrypted files on native, IndexedDB on web). Nothing here
//! talks to a server, and which conversations the HUD shows is the HUD's business.
//!
//! Store operations run one at a time, in order: a history read answers with every DM that was
//! emitted before it, including the one that made the HUD ask.

use std::collections::VecDeque;

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
use social::SocialClient;
use system_bridge::{DmHistoryEntryData, SystemApi};
use wallet::Wallet;

use super::dm_state::blocked_by_me;

enum DmStoreOp {
    Append {
        account: String,
        partner: String,
        entry: DmHistoryEntry,
    },
    Read {
        account: String,
        partner: String,
        sender: RpcResultSender<Vec<DmHistoryEntryData>>,
    },
    Delete {
        account: String,
        partner: String,
    },
}

/// Pending store operations and the one in flight.
#[derive(Resource, Default)]
pub struct DmStore {
    queue: VecDeque<DmStoreOp>,
    running: Option<Task<()>>,
}

impl DmStoreOp {
    async fn run(self) {
        match self {
            DmStoreOp::Append {
                account,
                partner,
                entry,
            } => {
                if let Err(e) = platform::dm_history_append(&account, &partner, &entry).await {
                    warn!("failed to store dm: {e}");
                }
            }
            DmStoreOp::Read {
                account,
                partner,
                sender,
            } => {
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
                            received_at: e.received_at,
                        })
                        .collect(),
                );
            }
            DmStoreOp::Delete { account, partner } => {
                if let Err(e) = platform::dm_history_delete(&account, &partner).await {
                    warn!("failed to delete dm history with {partner}: {e}");
                }
            }
        }
    }
}

/// Starts the next queued store operation once the previous one has finished.
pub fn run_dm_store(mut store: ResMut<DmStore>) {
    if let Some(mut task) = store.running.take() {
        if task.complete().is_none() {
            store.running = Some(task);
            return;
        }
    }
    if let Some(op) = store.queue.pop_front() {
        store.running = Some(IoTaskPool::get().spawn(op.run()));
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

/// Stores every DM sent or received under the local account, except from blocked senders.
pub fn record_dms(
    mut chats: EventReader<ChatEvent>,
    mut received: EventReader<PrivateChatReceived>,
    player: Query<Entity, With<PrimaryUser>>,
    wallet: Res<Wallet>,
    social: Res<SocialClient>,
    mut store: ResMut<DmStore>,
) {
    let Some(account) = wallet.address().map(|a| format!("{a:#x}")) else {
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
                received_at: now_unix(),
            },
        ));
    }
    for dm in received.read() {
        if blocked_by_me(&social, dm.from) {
            continue;
        }
        let partner = format!("{:#x}", dm.from);
        entries.push((
            partner.clone(),
            DmHistoryEntry {
                from: partner,
                message: dm.message.clone(),
                received_at: now_unix(),
            },
        ));
    }

    for (partner, entry) in entries {
        store.queue.push_back(DmStoreOp::Append {
            account: account.clone(),
            partner,
            entry,
        });
    }
}

pub fn handle_dm_history_requests(
    mut requests: EventReader<SystemApi>,
    wallet: Res<Wallet>,
    mut store: ResMut<DmStore>,
) {
    for request in requests.read() {
        match request {
            SystemApi::GetDmHistory(partner, sender) => {
                let sender = sender.clone();
                let (Some(account), Some(partner)) = (wallet.address(), normalize(partner)) else {
                    sender.send(Vec::new());
                    continue;
                };
                store.queue.push_back(DmStoreOp::Read {
                    account: format!("{account:#x}"),
                    partner,
                    sender,
                });
            }
            SystemApi::DeleteDmHistory(partner) => {
                let (Some(account), Some(partner)) = (wallet.address(), normalize(partner)) else {
                    continue;
                };
                store.queue.push_back(DmStoreOp::Delete {
                    account: format!("{account:#x}"),
                    partner,
                });
            }
            _ => (),
        }
    }
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
                .map(|e| format!("[{}] {}: {}", e.received_at, e.from, e.message))
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
