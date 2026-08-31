pub mod name_color;

use std::{io::Read, path::PathBuf, sync::Arc};

use alloy_core::primitives::Address;
use anyhow::anyhow;
use bevy::{
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use dcl::interface::CrdtType;
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer, IpfsIo};
use multihash_codetable::MultihashDigest;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    global_crdt::GlobalCrdtState,
    profile::name_color::{name_color_from_address, UNCLAIMED_NAME_COLOR},
};

use super::{
    broadcast,
    global_crdt::{process_transport_updates, ForeignPlayer, ProfileEvent, ProfileEventType},
    BroadcastTarget, NetworkMessage, NetworkMessageRecipient, ProfileUpdate, Transport,
    TransportType,
};
use common::{
    profile::{LambdaProfiles, SerializedProfile},
    rpc::RpcEventSender,
    sets::SceneSets,
    structs::PrimaryUser,
    util::{TaskCompat, TaskExt},
};
use common::{rpc::RpcCall, util::AsH160};
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4, sdk::components::PbPlayerIdentityData, Color3DclToBevy,
    },
    SceneComponentId, SceneEntityId,
};
use wallet::Wallet;

pub struct UserProfilePlugin;

impl Plugin for UserProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (drive_profile_fetches, request_missing_profiles).chain(),
                process_profile_events,
            )
                .before(process_transport_updates), // .in_set(TODO)
        );

        // a server has no real local player: never insert/announce/deploy the fake
        // player's profile or write PLAYER identity into scene crdts
        if !common::structs::server_mode() {
            app.add_systems(
                Update,
                setup_primary_profile.in_set(SceneSets::RestrictedActions), // in restricted actions so we get profile updates from login before a scene tick runs
            );
        }

        app.insert_resource(CurrentUserProfile::default());
        app.init_resource::<ProfileCache>()
            .init_resource::<ProfileMetaCache>()
            .add_event::<ProfileDeployedEvent>();
    }
}

/// Pacing for the fetch cascade: how long an unsatisfied entry waits after its last
/// cascade concluded before running another. Covers both failure retry (without it one
/// transient error would leave an address unresolvable for the engine's whole uptime)
/// and the convergence re-probe of data the registry hasn't confirmed yet.
const PROFILE_FETCH_RETRY: std::time::Duration = std::time::Duration::from_secs(30);
/// The registry serves many ids per request; keep a batch well under any server-side cap
/// and split anything longer across requests.
const PROFILE_REQUEST_BATCH: usize = 100;

/// Where held profile data came from. Declared in ascending order of authority so the
/// derived `Ord` ranks them: an equal-version answer only displaces held data when it
/// comes from a more authoritative source.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ProfileSource {
    Peer,
    Catalyst,
    Registry,
}

fn profile_is_guest(profile: &UserProfile) -> bool {
    !profile.content.has_connected_web3.unwrap_or(false)
}

#[derive(Default)]
struct ProfileEntry {
    /// highest version any announcement or response has claimed for this address
    announced: u32,
    /// best data we hold and where it came from
    data: Option<(Box<UserProfile>, ProfileSource)>,
    /// fetch strategy currently in flight (`Registry` = member of a batch POST,
    /// `Catalyst` = a single GET); p2p is fire-and-forget, tracked by `last_p2p`
    fetching: Option<ProfileSource>,
    /// when the next cascade may start; None = due now. Irrelevant once satisfied.
    next_fetch: Option<web_time::Instant>,
    /// `time.elapsed_secs()` of the last `ProfileRequest` sent to the peer
    last_p2p: Option<f32>,
}

impl ProfileEntry {
    fn profile(&self) -> Option<&UserProfile> {
        self.data.as_ref().map(|(profile, _)| profile.as_ref())
    }

    /// nothing better to look for: registry-confirmed data, or a guest peer's own
    /// answer (which no registry will ever hold), at the announced version
    fn satisfied(&self) -> bool {
        self.data.as_ref().is_some_and(|(profile, source)| {
            profile.version >= self.announced
                && (*source == ProfileSource::Registry
                    || (*source == ProfileSource::Peer && profile_is_guest(profile)))
        })
    }

    fn due(&self, now: web_time::Instant) -> bool {
        self.next_fetch.is_none_or(|at| at <= now)
    }

    fn wants_fetch(&self, now: web_time::Instant) -> bool {
        !self.satisfied() && self.fetching.is_none() && self.due(now)
    }

    /// catalyst is worth asking when we hold nothing, hold only p2p data, or are behind
    /// the announcement; catalyst-sourced data at the announced version re-probes the
    /// registry alone
    fn include_catalyst(&self) -> bool {
        match &self.data {
            None => true,
            Some((profile, _)) if profile.version < self.announced => true,
            Some((_, ProfileSource::Peer)) => true,
            _ => false,
        }
    }

    /// accept a newer version from anywhere, and an equal version only from a more
    /// authoritative source; also lifts `announced` so a response can't be outranked by
    /// an announcement we haven't seen
    fn apply(&mut self, profile: UserProfile, source: ProfileSource) -> bool {
        self.announced = self.announced.max(profile.version);
        let improvement = match &self.data {
            None => true,
            Some((held, held_source)) => {
                profile.version > held.version
                    || (profile.version == held.version && source > *held_source)
            }
        };
        if improvement {
            self.data = Some((Box::new(profile), source));
        }
        improvement
    }

    /// ask the peer directly when we're missing or behind the announcement: for guests
    /// this is the only source there is, for everyone else the peer is the authority on
    /// its own latest profile. Held back until the first cascade concludes so the
    /// authoritative sources get first go, except when we already hold (stale) data —
    /// then the request rides alongside the re-fetch.
    fn wants_p2p(&self, now: f32) -> bool {
        let behind = self
            .data
            .as_ref()
            .is_none_or(|(profile, _)| profile.version < self.announced);
        if !behind {
            return false;
        }
        let concluded = self.fetching.is_none() && self.next_fetch.is_some();
        if self.data.is_none() && !concluded {
            return false;
        }
        self.last_p2p.is_none_or(|at| now - at >= 10.0)
    }
}

type CatalystFetch = Task<Result<Option<UserProfile>, anyhow::Error>>;
type RegistryBatch = Task<Vec<(Address, Result<Option<UserProfile>, anyhow::Error>)>>;

#[derive(Resource, Default)]
pub struct ProfileCache {
    entries: HashMap<Address, ProfileEntry>,
    registry_batches: Vec<RegistryBatch>,
    catalyst_fetches: HashMap<Address, CatalystFetch>,
}

#[derive(Resource, Default)]
pub struct ProfileMetaCache(pub HashMap<Address, String>);

#[derive(SystemParam)]
pub struct ProfileManager<'w, 's> {
    cache: ResMut<'w, ProfileCache>,
    meta_cache: ResMut<'w, ProfileMetaCache>,
    ipfs: IpfsAssetServer<'w, 's>,
}

pub struct ProfileMissingError;

impl ProfileManager<'_, '_> {
    /// Serve the best data we hold (however stale); creating the entry queues the fetch
    /// cascade, which `drive_profile_fetches` runs. Ok(None) while a fetch is pending;
    /// Err once a cascade has concluded with nothing (until the retry comes due).
    pub fn get_data(
        &mut self,
        address: Address,
    ) -> Result<Option<&UserProfile>, ProfileMissingError> {
        let entry = self.cache.entries.entry(address).or_default();
        if entry.data.is_none() && entry.fetching.is_none() && !entry.due(web_time::Instant::now())
        {
            return Err(ProfileMissingError);
        }
        Ok(entry.profile())
    }

    /// Record that this address' profile is claimed to exist at `version`; a version
    /// ahead of what we hold makes the fetch cascade due immediately.
    pub fn announce(&mut self, address: Address, version: u32) {
        let entry = self.cache.entries.entry(address).or_default();
        if version > entry.announced {
            entry.announced = version;
            entry.next_fetch = None;
        }
    }

    pub fn get_image(
        &mut self,
        address: Address,
    ) -> Result<Option<Handle<Image>>, ProfileMissingError> {
        let profile = self.get_data(address)?;
        let Some(profile) = profile else {
            return Ok(None);
        };
        let Some(path) = profile
            .content
            .avatar
            .snapshots
            .as_ref()
            .and_then(|snapshots| {
                if snapshots.face256.is_empty() {
                    None
                } else {
                    // let url = format!("{}{}", profile.base_url, snapshots.face256);
                    let ipfs_path = IpfsPath::new_from_url(&snapshots.face256, "png");
                    Some(PathBuf::from(&ipfs_path))
                }
            })
        else {
            return Err(ProfileMissingError);
        };
        Ok(Some(self.ipfs.asset_server().load(path)))
    }

    pub fn get_name(&mut self, address: Address) -> Result<Option<&String>, ProfileMissingError> {
        Ok(self.get_data(address)?.map(|profile| &profile.content.name))
    }

    /// Record an authoritative profile (our own, which we deploy ourselves).
    /// Unconditional: even a same-version edit replaces what the cache holds.
    pub fn update(&mut self, profile: UserProfile) {
        if let Some(address) = profile.content.eth_address.as_h160() {
            let entry = self.cache.entries.entry(address).or_default();
            entry.announced = entry.announced.max(profile.version);
            entry.data = Some((Box::new(profile), ProfileSource::Registry));
        }
    }

    /// Record a profile received directly from the owning peer. `apply`'s authority
    /// ranking keeps a room-broadcast response from displacing an equal-version
    /// registry answer; a guest's own answer counts as final via `satisfied`.
    pub fn update_from_peer(&mut self, profile: UserProfile) {
        if let Some(address) = profile.content.eth_address.as_h160() {
            self.cache
                .entries
                .entry(address)
                .or_default()
                .apply(profile, ProfileSource::Peer);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn setup_primary_profile(
    mut commands: Commands,
    player: Query<(Entity, Option<&UserProfile>), With<PrimaryUser>>,
    mut current_profile: ResMut<CurrentUserProfile>,
    transports: Query<&Transport>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
    mut deploy_task: Local<Option<(u32, Task<Result<(), anyhow::Error>>)>>,
    wallet: Res<Wallet>,
    ipfas: IpfsAssetServer,
    mut contexts: Query<&mut GlobalCrdtState>,
    mut cache: ProfileManager,
    mut last_announce: Local<f32>,
    time: Res<Time>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribeProfileChanged { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    if let Ok((player, maybe_profile)) = player.single() {
        if maybe_profile.is_none() || current_profile.is_changed() {
            let Some(profile) = current_profile.profile.as_ref() else {
                commands.entity(player).remove::<UserProfile>();
                return;
            };

            // update component
            commands.entity(player).try_insert(profile.clone());

            // update cache
            cache.update(profile.clone());

            // send to scenes (every context: the local player is `PLAYER` in all of them)
            for mut global_crdt in contexts.iter_mut() {
                global_crdt.update_crdt(
                    SceneComponentId::PLAYER_IDENTITY_DATA,
                    CrdtType::LWW_ANY,
                    SceneEntityId::PLAYER,
                    &PbPlayerIdentityData {
                        address: profile.content.eth_address.clone(),
                        is_guest: !(profile.content.has_connected_web3.unwrap_or(false)),
                    },
                );
            }

            // Announce the new version over Pulse alone, as a `ProfileVersionAnnouncement` — peers
            // refetch from catalyst, or ask us directly with an rfc4 `ProfileRequest`, which is still
            // answered on the byte transports (that request/response pair is what resolves a guest,
            // who has no catalyst presence). No byte transport carries the unsolicited announcement:
            // a peer that sees one over LiveKit answers it with LiveKit movement, which is exactly
            // the traffic Pulse replaces.
            // Reset the keepalive timer so the periodic re-announce below doesn't immediately fire
            // again.
            debug!("announcing profile new version {:?}", profile.version);
            broadcast(
                transports.iter(),
                BroadcastTarget::PULSE,
                false,
                ProfileUpdate {
                    serialized_profile: serde_json::to_string(&profile.content).unwrap(),
                    base_url: profile.base_url.clone(),
                    version: profile.version,
                },
            );
            *last_announce = time.elapsed_secs();

            // send to event receivers
            senders.retain(|sender| {
                let _ = sender.send(format!(
                    "{{ \"ethAddress\": \"{}\", \"version\": \"{}\" }}",
                    profile.content.user_id.as_ref().unwrap(),
                    profile.version
                ));
                !sender.is_closed()
            });

            // deploy to server
            if !current_profile.is_deployed {
                debug!("deploying {:#?}", profile);
                let ipfs = ipfas.ipfs().clone();
                let profile = profile.clone();
                let wallet = wallet.clone();
                *deploy_task = Some((
                    profile.version,
                    IoTaskPool::get().spawn_compat(deploy_profile(ipfs, wallet, profile)),
                ));
                current_profile.is_deployed = true;
            }
        } else if let Some(current_profile) = current_profile.profile.as_ref() {
            let now = time.elapsed_secs();
            if now > *last_announce + 5.0 {
                debug!("announcing profile v {}", current_profile.version);
                // The keepalive re-announce goes the same way as the version bump above: Pulse
                // only, converted to a `ProfileVersionAnnouncement`.
                broadcast(
                    transports.iter(),
                    BroadcastTarget::PULSE,
                    false,
                    rfc4::AnnounceProfileVersion {
                        profile_version: current_profile.version,
                    },
                );
                *last_announce = now;
            }
        }
    }

    if let Some((version, mut task)) = deploy_task.take() {
        match task.complete() {
            Some(Ok(())) => {
                info!("deployed profile ok");
                commands.send_event(ProfileDeployedEvent {
                    version,
                    success: true,
                });
            }
            Some(Err(e)) => {
                error!("failed to deploy profile: {e}");
                commands.send_event(ProfileDeployedEvent {
                    version,
                    success: false,
                });
                // todo toast
            }
            None => *deploy_task = Some((version, task)),
        }
    }
}

#[derive(Resource, Default)]
pub struct CurrentUserProfile {
    pub profile: Option<UserProfile>,
    pub snapshots: Option<(Handle<Image>, Handle<Image>)>,
    pub is_deployed: bool,
}

/// Run the fetch cascade for every entry that wants one: registry batches (grouped by
/// registry host, since .zone and .org hold separate namespaces), escalating a registry
/// miss to a single catalyst fetch where that's worth asking, and pacing unsatisfied
/// entries with `PROFILE_FETCH_RETRY`. All fetch policy lives here — the fetchers
/// themselves are dumb HTTP.
fn drive_profile_fetches(mut manager: ProfileManager) {
    let ProfileManager {
        cache,
        meta_cache,
        ipfs,
    } = &mut manager;
    let ProfileCache {
        entries,
        registry_batches,
        catalyst_fetches,
    } = &mut **cache;
    let own_endpoint = ipfs.ipfs().lambda_endpoint();

    // apply landed registry batches, escalating misses to a catalyst fetch
    registry_batches.retain_mut(|batch| {
        let Some(results) = batch.complete() else {
            return true;
        };
        for (address, result) in results {
            let Some(entry) = entries.get_mut(&address) else {
                continue;
            };
            entry.fetching = None;
            match result {
                Ok(Some(profile)) => {
                    debug!("applying registry response for {address:#x}");
                    entry.apply(profile, ProfileSource::Registry);
                }
                Ok(None) => debug!("registry has no profile for {address:#x}"),
                Err(e) => {
                    if entry.data.is_none() {
                        warn!("profile fetch failed for {address:#x}: {e}");
                    } else {
                        debug!("profile re-fetch failed for {address:#x}: {e}");
                    }
                }
            }
            if entry.satisfied() {
                continue;
            }
            let endpoint = meta_cache
                .0
                .get(&address)
                .cloned()
                .or_else(|| own_endpoint.clone());
            if let Some(endpoint) = entry.include_catalyst().then_some(endpoint).flatten() {
                entry.fetching = Some(ProfileSource::Catalyst);
                catalyst_fetches.insert(
                    address,
                    IoTaskPool::get().spawn_compat(fetch_catalyst_profile(
                        endpoint,
                        address,
                        ipfs.ipfs().clone(),
                    )),
                );
            } else {
                entry.next_fetch = Some(web_time::Instant::now() + PROFILE_FETCH_RETRY);
            }
        }
        false
    });

    // apply landed catalyst fetches; the cascade concludes here either way
    catalyst_fetches.retain(|address, task| {
        let Some(result) = task.complete() else {
            return true;
        };
        if let Some(entry) = entries.get_mut(address) {
            entry.fetching = None;
            match result {
                Ok(Some(profile)) => {
                    entry.apply(profile, ProfileSource::Catalyst);
                }
                Ok(None) => debug!("catalyst has no profile for {address:#x}"),
                Err(e) => {
                    if entry.data.is_none() {
                        warn!("profile fetch failed for {address:#x}: {e}");
                    } else {
                        debug!("profile re-fetch failed for {address:#x}: {e}");
                    }
                }
            }
            if !entry.satisfied() {
                entry.next_fetch = Some(web_time::Instant::now() + PROFILE_FETCH_RETRY);
            }
        }
        false
    });

    // collect due entries into registry batches, one set per registry host
    let now = web_time::Instant::now();
    let mut wants: HashMap<&'static str, Vec<Address>> = HashMap::default();
    for (address, entry) in entries.iter_mut() {
        if entry.wants_fetch(now) {
            entry.fetching = Some(ProfileSource::Registry);
            let endpoint = meta_cache
                .0
                .get(address)
                .map(String::as_str)
                .or(own_endpoint.as_deref());
            wants
                .entry(registry_url(endpoint))
                .or_default()
                .push(*address);
        }
    }
    for (url, addresses) in wants {
        for chunk in addresses.chunks(PROFILE_REQUEST_BATCH) {
            registry_batches.push(IoTaskPool::get().spawn_compat(fetch_registry_profiles(
                url,
                chunk.to_vec(),
                ipfs.ipfs().clone(),
            )));
        }
    }
}

fn request_missing_profiles(
    mut commands: Commands,
    missing: Query<(Entity, &ForeignPlayer), Without<UserProfile>>,
    versioned: Query<(Entity, &ForeignPlayer, &UserProfile)>,
    mut manager: ProfileManager,
    mut contexts: Query<&mut GlobalCrdtState>,
    transports: Query<&Transport>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();

    // resolved players: push cache movement, but diff before any push — only real
    // changes reach the entity/scenes
    for (ent, player, current) in versioned.iter() {
        manager.announce(player.address, player.profile_version);
        let Some(entry) = manager.cache.entries.get_mut(&player.address) else {
            continue;
        };

        if let Some(data) = entry.profile() {
            if data.version >= current.version && data != current {
                let was_guest = profile_is_guest(current);
                let is_guest = profile_is_guest(data);
                if was_guest != is_guest {
                    if let Ok(mut global_crdt) = contexts.get_mut(player.context) {
                        global_crdt.update_crdt(
                            SceneComponentId::PLAYER_IDENTITY_DATA,
                            CrdtType::LWW_ANY,
                            player.scene_id,
                            &PbPlayerIdentityData {
                                address: format!("{:#x}", player.address),
                                is_guest,
                            },
                        );
                    }
                }
                commands.entity(ent).try_insert(data.clone());
            }
        }

        if entry.wants_p2p(now) && send_profile_request(player, &transports) {
            entry.last_p2p = Some(now);
        }
    }

    // unresolved players: push the first data that lands, even when it's older than the
    // announced version — a name beats a truncated address, and the fetch cascade keeps
    // working toward the announced version
    for (ent, player) in missing.iter() {
        manager.announce(player.address, player.profile_version);
        let Some(entry) = manager.cache.entries.get_mut(&player.address) else {
            continue;
        };

        if let Some(data) = entry.profile() {
            if let Ok(mut global_crdt) = contexts.get_mut(player.context) {
                global_crdt.update_crdt(
                    SceneComponentId::PLAYER_IDENTITY_DATA,
                    CrdtType::LWW_ANY,
                    player.scene_id,
                    &PbPlayerIdentityData {
                        address: format!("{:#x}", player.address),
                        is_guest: profile_is_guest(data),
                    },
                );
            }
            commands.entity(ent).try_insert(data.clone());
        }

        if entry.wants_p2p(now) && send_profile_request(player, &transports) {
            entry.last_p2p = Some(now);
        }
    }
}

/// Pick a transport at request time rather than remembering one. `player.transports` is the
/// set this peer is actually on — joined rooms are added, departures remove them — so any
/// non-Pulse member is a channel we provably share; Pulse itself is excluded because it
/// relays version announcements only (its bridge can't encode a `ProfileRequest`). Nothing
/// is cached, so a room joined later is picked up on the next retry.
fn send_profile_request(player: &ForeignPlayer, transports: &Query<&Transport>) -> bool {
    let Some(transport) = player
        .transports
        .iter()
        .filter_map(|transport| transports.get(*transport).ok())
        .find(|t| t.transport_type != TransportType::Pulse)
    else {
        return false;
    };

    let request = rfc4::Packet {
        message: Some(rfc4::packet::Message::ProfileRequest(
            rfc4::ProfileRequest {
                address: format!("{:#x}", player.address),
                profile_version: player.profile_version,
            },
        )),
        protocol_version: 100,
    };
    match transport.sender.try_send(NetworkMessage {
        recipient: NetworkMessageRecipient::Peer(player.address),
        ..NetworkMessage::unreliable(&request)
    }) {
        Err(e) => {
            warn!("failed to send request: {e}");
        }
        Ok(_) => {
            debug!("sent profile request for player {player:?}");
        }
    };
    true
}

#[allow(clippy::too_many_arguments)]
pub fn process_profile_events(
    mut commands: Commands,
    mut players: Query<(&mut ForeignPlayer, Option<&mut UserProfile>)>,
    mut events: EventReader<ProfileEvent>,
    mut last_sent_request: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
    wallet: Res<Wallet>,
    transports: Query<&Transport>,
    current_profile: Res<CurrentUserProfile>,
    mut contexts: Query<&mut GlobalCrdtState>,
    mut cache: ProfileManager,
) {
    for ev in events.read() {
        match &ev.event {
            ProfileEventType::Request {
                request: r,
                transport: request_transport,
            } => {
                if let Some(req_address) = r.address.as_h160() {
                    if Some(req_address) == wallet.address() {
                        // Answer on the transport the request arrived on — the one channel we know
                        // we share with the asker. Still `recipient: All`: anyone else in that room
                        // needing our profile gets it for free, which is what the per-transport
                        // debounce below is rate-limiting.
                        let Ok(transport) = transports.get(*request_transport) else {
                            debug!("not sending profile, no transport");
                            continue;
                        };

                        if last_sent_request.get(request_transport).is_some() {
                            debug!("ignoring request for my profile (sent recently)");
                            continue;
                        }

                        let Some(current_profile) = current_profile.profile.as_ref() else {
                            return;
                        };

                        debug!("sending my profile");
                        let response = rfc4::Packet {
                            message: Some(rfc4::packet::Message::ProfileResponse(
                                rfc4::ProfileResponse {
                                    serialized_profile: serde_json::to_string(
                                        &current_profile.content,
                                    )
                                    .unwrap(),
                                    base_url: current_profile.base_url.clone(),
                                },
                            )),
                            protocol_version: 100,
                        };
                        let _ = transport
                            .sender
                            .try_send(NetworkMessage::reliable(&response));
                        last_sent_request.insert(*request_transport, time.elapsed_secs());
                    }
                }
            }
            ProfileEventType::Version(v) => {
                if let Ok((mut player, _)) = players.get_mut(ev.sender) {
                    if player.profile_version != v.profile_version {
                        player.profile_version = v.profile_version;
                    }
                    // a version ahead of what we hold makes the fetch cascade due
                    cache.announce(player.address, v.profile_version);
                } else {
                    warn!("profile version for unknown player {:?}", ev.sender);
                }
            }
            ProfileEventType::Response(r) => {
                if let Ok((mut player, maybe_profile)) = players.get_mut(ev.sender) {
                    let serialized_profile: SerializedProfile =
                        match serde_json::from_str(&r.serialized_profile) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!("failed to parse profile: {e}");
                                continue;
                            }
                        };
                    let version = serialized_profile.version as u32;

                    // check/update profile version
                    if version < player.profile_version {
                        continue;
                    }
                    if version > player.profile_version {
                        player.profile_version = version;
                    }

                    let profile = UserProfile {
                        version,
                        content: serialized_profile,
                        base_url: r.base_url.clone(),
                    };

                    // diff before any push: only write the identity crdt when the guest
                    // flag actually changes (or on first sight of this player's profile)
                    let is_guest = !(profile.content.has_connected_web3.unwrap_or(false));
                    let guest_changed = maybe_profile.as_ref().is_none_or(|existing| {
                        let was_guest = !existing.content.has_connected_web3.unwrap_or(false);
                        was_guest != is_guest
                    });
                    if guest_changed {
                        if let Ok(mut global_crdt) = contexts.get_mut(player.context) {
                            global_crdt.update_crdt(
                                SceneComponentId::PLAYER_IDENTITY_DATA,
                                CrdtType::LWW_ANY,
                                player.scene_id,
                                &PbPlayerIdentityData {
                                    address: format!("{:#x}", player.address),
                                    is_guest,
                                },
                            );
                        }
                    }

                    cache.update_from_peer(profile.clone());

                    if let Some(mut existing_profile) = maybe_profile {
                        if existing_profile.as_ref() != &profile {
                            *existing_profile = profile;
                        }
                    } else {
                        commands.entity(ev.sender).try_insert(profile);
                    }
                } else {
                    warn!("profile update for unknown player {:?}", ev.sender);
                }
            }
        }
    }

    last_sent_request.retain(|_, req_time| *req_time > time.elapsed_secs() - 10.0);
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct UserProfile {
    pub version: u32,
    pub content: SerializedProfile,
    pub base_url: String,
}

impl UserProfile {
    pub fn is_female(&self) -> bool {
        self.content
            .avatar
            .body_shape
            .as_ref()
            .and_then(|s| s.rsplit(':').next())
            .is_none_or(|shape| shape.to_lowercase() == "basefemale")
    }

    pub fn name_color(&self) -> Color {
        if !self.content.has_claimed_name {
            UNCLAIMED_NAME_COLOR
        } else if let Some(custom_name_color) = self.content.name_color {
            custom_name_color.convert_srgb()
        } else {
            name_color_from_address(self.content.eth_address.as_h160().unwrap_or_default())
        }
    }
}

#[derive(Serialize)]
pub struct Deployment<'a> {
    version: &'a str,
    #[serde(rename = "type")]
    ty: &'a str,
    pointers: Vec<String>,
    timestamp: u128,
    metadata: serde_json::Value,
}

#[derive(Event)]
pub struct ProfileDeployedEvent {
    pub version: u32,
    pub success: bool,
}

async fn deploy_profile(
    ipfs: Arc<IpfsIo>,
    wallet: Wallet,
    mut profile: UserProfile,
) -> Result<(), anyhow::Error> {
    let unix_time = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    profile.content.avatar.snapshots = None;

    let deployment = serde_json::to_string(&Deployment {
        version: "v3",
        ty: "profile",
        pointers: vec![profile.content.eth_address.clone()],
        timestamp: unix_time,
        metadata: serde_json::json!({
            "avatars": [
                profile.content
            ]
        }),
    })?;

    let post = {
        let hash = multihash_codetable::Code::Sha2_256.digest(deployment.as_bytes());
        let cid = cid::Cid::new_v1(0x55, hash).to_string();
        let profile_chain = wallet.sign_message(cid.clone()).await?;

        let mut form_data = multipart::client::lazy::Multipart::new();
        form_data.add_text("entityId", cid.clone());
        for (key, data) in profile_chain.formdata() {
            form_data.add_text(key, data);
        }
        form_data.add_stream(
            cid,
            std::io::Cursor::new(deployment.into_bytes()),
            Option::<&str>::None,
            None,
        );

        let mut prepared = form_data.prepare()?;
        let mut prepared_data = Vec::default();
        prepared.read_to_end(&mut prepared_data)?;

        let url = ipfs
            .entities_endpoint()
            .ok_or_else(|| anyhow!("no entities endpoint"))?;
        debug!("deploying to {url}");

        ipfs.client()
            .post(url)
            // multipart deploy carries profile snapshots, so allow more headroom
            .timeout(std::time::Duration::from_secs(60))
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", prepared.boundary()),
            )
            .body(prepared_data)
    };

    let response = post.send().await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        _ => Err(anyhow!(
            "bad response: {}: {}",
            response.status(),
            String::from_utf8_lossy(&response.bytes().await?)
        )),
    }
}

const REGISTRY_ORG: &str = "https://asset-bundle-registry.decentraland.org/profiles";
const REGISTRY_ZONE: &str = "https://asset-bundle-registry.decentraland.zone/profiles";

/// The .zone and .org registries hold separate profile namespaces, so the registry must
/// match the profile owner's environment, judged by their announced lambdas endpoint:
/// a host under the zone tld uses the zone registry.
fn registry_url(endpoint: Option<&str>) -> &'static str {
    let is_zone = endpoint
        .and_then(|e| reqwest::Url::parse(e).ok())
        .is_some_and(|url| url.host_str().and_then(|host| host.rsplit('.').next()) == Some("zone"));
    if is_zone {
        REGISTRY_ZONE
    } else {
        REGISTRY_ORG
    }
}

/// One registry POST for a chunk of ids, reported per item: Ok(Some) found, Ok(None)
/// authoritatively absent, Err a chunk-wide transport/parse failure. Results only cover
/// the addresses that were asked for — the response identifies each profile by its
/// deployed metadata, which is written by whoever deployed it.
async fn fetch_registry_profiles(
    registry_url: &'static str,
    addresses: Vec<Address>,
    ipfs: std::sync::Arc<IpfsIo>,
) -> Vec<(Address, Result<Option<UserProfile>, anyhow::Error>)> {
    let base_url = ipfs.contents_endpoint().unwrap_or_default();
    let ids = addresses
        .iter()
        .map(|address| format!("{address:#x}"))
        .collect::<Vec<_>>();
    debug!("requesting {} profiles from {registry_url}", ids.len());

    let outcome: Result<Vec<LambdaProfiles>, anyhow::Error> = async {
        let response = ipfs
            .client()
            .post(registry_url)
            .timeout(std::time::Duration::from_secs(10))
            .body(serde_json::json!({ "ids": ids }).to_string())
            .header("content-type", "application/json")
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            anyhow::bail!("bad response: {}", response.status());
        }
        Ok(response.json::<Vec<LambdaProfiles>>().await?)
    }
    .await;

    match outcome {
        Err(e) => {
            let message = format!("registry fetch from {registry_url}: {e}");
            addresses
                .into_iter()
                .map(|address| (address, Err(anyhow!(message.clone()))))
                .collect()
        }
        Ok(batches) => {
            let mut found = HashMap::<Address, SerializedProfile>::default();
            for batch in batches {
                let Some(content) = batch.avatars.into_iter().next() else {
                    continue;
                };
                let Some(address) = content.eth_address.as_h160() else {
                    continue;
                };
                // never let a later entry displace an earlier one
                found.entry(address).or_insert(content);
            }
            addresses
                .into_iter()
                .map(|address| {
                    let profile = found.remove(&address).map(|content| UserProfile {
                        version: content.version as u32,
                        content,
                        base_url: base_url.clone(),
                    });
                    (address, Ok(profile))
                })
                .collect()
        }
    }
}

/// One catalyst lambdas GET for one address: Ok(None) means the catalyst
/// authoritatively has no profile.
async fn fetch_catalyst_profile(
    endpoint: String,
    address: Address,
    ipfs: std::sync::Arc<IpfsIo>,
) -> Result<Option<UserProfile>, anyhow::Error> {
    let url = format!("{}/profiles/{address:#x}", endpoint.trim_end_matches('/'));
    debug!("requesting profile from {url}");

    let response = ipfs
        .client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| anyhow!("catalyst fetch from {url}: {e}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if response.status() != StatusCode::OK {
        anyhow::bail!(
            "catalyst fetch from {url}: bad response: {}",
            response.status()
        );
    }

    let content = response.json::<LambdaProfiles>().await?;
    let base_url = ipfs.contents_endpoint().unwrap_or_default();
    Ok(content
        .avatars
        .into_iter()
        .next()
        .map(|content| UserProfile {
            version: content.version as u32,
            content,
            base_url,
        }))
}

/// Resolve one profile through the registry -> catalyst chain without the engine cache
/// (used at login, before any player entities exist). Ok(None) means both sources
/// authoritatively report no profile; a failure with no other source answering is an
/// Err, so callers can tell "no profile" from "couldn't fetch".
pub async fn get_remote_profile(
    address: Address,
    ipfs: std::sync::Arc<IpfsIo>,
    endpoint: Option<String>,
) -> Result<Option<UserProfile>, anyhow::Error> {
    let endpoint = endpoint.or_else(|| ipfs.lambda_endpoint());

    let mut fetch_error = None;
    match fetch_registry_profiles(
        registry_url(endpoint.as_deref()),
        vec![address],
        ipfs.clone(),
    )
    .await
    .pop()
    {
        Some((_, Ok(Some(profile)))) => return Ok(Some(profile)),
        Some((_, Err(e))) => fetch_error = Some(e),
        _ => (),
    }

    if let Some(endpoint) = endpoint {
        match fetch_catalyst_profile(endpoint, address, ipfs).await {
            Ok(Some(profile)) => return Ok(Some(profile)),
            Ok(None) => (),
            Err(e) => {
                if fetch_error.is_none() {
                    fetch_error = Some(e);
                }
            }
        }
    } else if fetch_error.is_none() {
        fetch_error = Some(anyhow!("not connected"));
    }

    match fetch_error {
        Some(e) => Err(e),
        None => Ok(None),
    }
}
