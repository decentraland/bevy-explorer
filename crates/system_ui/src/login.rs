use std::str::FromStr;

use alloy_core::primitives::{Address, B256};
use alloy_signer_local::PrivateKeySigner;
use analytics::segment_system::SegmentConfig;
use bevy::{
    app::AppExit,
    prelude::*,
    tasks::{IoTaskPool, Task},
    window::PrimaryWindow,
};
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::SerializedProfile,
    rpc::{RpcResultReceiver, RpcResultSender},
    sets::SceneSets,
    structs::{
        ActiveDialog, AppConfig, ChainLink, CurrentRealm, DialogPermit, PreviousLogin, SystemAudio,
        WorldHold, ZOrder, TERMS_VERSION,
    },
    util::{TaskCompat, TaskExt},
};
use comms::profile::{
    get_default_look, get_remote_profile, CurrentUserProfile, UserProfile, DEFAULT_LOOKS,
};
use ipfs::{IpfsAssetServer, IpfsIo};
use scene_runner::Toaster;
use system_bridge::{NativeUi, SystemApi, WelcomeState, PROFILE_FETCH_FAILED};
use tokio::sync::oneshot::error::TryRecvError;
use ui_core::{
    button::DuiButton,
    ui_actions::{close_ui_happy, Click, EventCloneExt, On},
};
use wallet::{
    browser_auth::{finish_remote_ephemeral_request, init_remote_ephemeral_request, EphemeralKey},
    Wallet,
};

use crate::version_check::check_update;

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoginType>().add_systems(
            Update,
            (
                (login, update_profile_for_realm).run_if(in_state(ui_core::State::Ready)),
                process_login_bridge.in_set(SceneSets::PostLoop), // use post loop here so that the PlayerIdentityData can be picked up in RestrictedActions
            ),
        );
    }
}

#[derive(Event, Clone)]
enum LoginType {
    ExistingRemote,
    NewRemote,
    Guest,
    Cancel,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn login(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut req_code: Local<Option<RpcResultReceiver<Result<Option<i32>, String>>>>,
    mut req_done: Local<Option<RpcResultReceiver<Result<(), String>>>>,
    mut logins: EventReader<LoginType>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
    dui: Res<DuiRegistry>,
    active_dialog: Res<ActiveDialog>,
    mut motd_shown: Local<bool>,
    mut update_check: Local<Option<bevy::tasks::Task<Option<(String, String)>>>>,
    mut bridge: EventWriter<SystemApi>,
    native_active: Res<NativeUi>,
    config: Res<AppConfig>,
) {
    if !native_active.login {
        return;
    }

    if !*motd_shown {
        // don't block the main thread on the github release check
        let task = update_check.get_or_insert_with(|| IoTaskPool::get().spawn(check_update()));
        let Some(update) = task.complete() else {
            return;
        };
        *update_check = None;
        let permit = active_dialog.try_acquire().unwrap();

        if let Some((desc, url)) = update {
            let components = commands
                .spawn_template(
                    &dui,
                    "update-available",
                    DuiProps::new()
                        .with_prop("download", url)
                        .with_prop("body", desc)
                        .with_prop("buttons", vec![DuiButton::new_enabled("Ok", (|mut commands: Commands, dui: Res<DuiRegistry>, mut permit: Query<&mut DialogPermit>| {
                            let mut permit = permit.single_mut().unwrap();
                            let permit = permit.take();
                            let components = commands
                                .spawn_template(
                                    &dui,
                                    "motd",
                                    DuiProps::default()
                                        .with_prop("buttons", vec![DuiButton::new_enabled("Ok", close_ui_happy)]),
                                )
                                .unwrap();
                            commands.entity(components.root).try_insert((permit, ZOrder::Login.default()));
                        }).pipe(close_ui_happy))]),
                )
                .unwrap();
            commands
                .entity(components.root)
                .try_insert((permit, ZOrder::Login.default()));
        } else {
            let components = commands
                .spawn_template(
                    &dui,
                    "motd",
                    DuiProps::default().with_prop(
                        "buttons",
                        vec![DuiButton::new_enabled("Ok", close_ui_happy)],
                    ),
                )
                .unwrap();
            commands
                .entity(components.root)
                .try_insert((permit, ZOrder::Login.default()));
        }
        *motd_shown = true;
        return;
    }

    // cleanup if we're done
    if wallet.address().is_some() {
        if let Some(mut commands) = dialog.and_then(|d| commands.get_entity(d).ok()) {
            commands.despawn();
        }
        *dialog = None;
        *req_code = None;
        *req_done = None;
        return;
    }

    // create dialog
    if dialog.is_none() && req_done.is_none() {
        let Some(permit) = active_dialog.try_acquire() else {
            return;
        };

        let previous_login = get_previous_login(&config);

        let mut dlg = commands.spawn(permit);
        *dialog = Some(dlg.id());
        dlg.apply_template(
            &dui,
            "login",
            DuiProps::new()
                .with_prop("allow-reuse", previous_login.is_some())
                .with_prop("reuse", LoginType::ExistingRemote.send_value_on::<Click>())
                .with_prop("connect", LoginType::NewRemote.send_value_on::<Click>())
                .with_prop("guest", LoginType::Guest.send_value_on::<Click>())
                .with_prop(
                    "quit",
                    On::<Click>::new(|mut e: EventWriter<AppExit>| {
                        e.write_default();
                    }),
                ),
        )
        .unwrap();

        return;
    }

    // handle task results
    if let Some(mut t) = req_code.take() {
        match t.try_recv() {
            Ok(Ok(_)) => {
                // no code to show: the waiting dialog spawned on click stays up
            }
            Ok(Err(e)) => {
                toaster.add_toast("login profile", format!("Login failed: {e}"));
                if let Some(mut commands) = dialog.and_then(|d| commands.get_entity(d).ok()) {
                    commands.despawn();
                    *dialog = None;
                }
            }
            Err(TryRecvError::Empty) => {
                *req_code = Some(t);
            }
            Err(e) => {
                warn!("unexpected {e}");
            }
        }
    }

    if let Some(mut t) = req_done.take() {
        match t.try_recv() {
            Ok(Ok(())) => {
                *dialog = None;
            }
            Ok(Err(e)) => {
                error!("{e}");
                toaster.add_toast("login profile", format!("Login failed: {e}"));
                if let Some(mut commands) = dialog.and_then(|d| commands.get_entity(d).ok()) {
                    commands.despawn();
                }
                *dialog = None;
            }
            Err(TryRecvError::Empty) => {
                *req_done = Some(t);
            }
            Err(e) => {
                warn!("unexpected {e}");
            }
        }
    }

    // handle click
    if let Some(login) = logins.read().last() {
        if let Some(mut commands) = dialog.and_then(|d| commands.get_entity(d).ok()) {
            commands.despawn();
            *dialog = None;
        }

        match login {
            LoginType::ExistingRemote => {
                info!("existing remote");
                commands.send_event(SystemAudio(
                    "embedded://sounds/ui/toggle_enable.wav".to_owned(),
                ));
                let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();
                bridge.write(SystemApi::LoginPrevious(false, sx));
                *req_done = Some(rx);
            }
            LoginType::NewRemote => {
                info!("new remote");

                commands.send_event(SystemAudio(
                    "embedded://sounds/ui/toggle_enable.wav".to_owned(),
                ));
                let (scode, rcode) = RpcResultSender::<Result<Option<i32>, String>>::channel();
                let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();
                bridge.write(SystemApi::LoginNew(false, scode, sx));
                *req_code = Some(rcode);
                *req_done = Some(rx);

                let components = commands
                    .spawn(ZOrder::Login.default())
                    .apply_template(
                        &dui,
                        "cancel-login",
                        DuiProps::new().with_prop(
                            "buttons",
                            vec![DuiButton::new_enabled(
                                "Cancel",
                                |mut e: EventWriter<LoginType>| {
                                    e.write(LoginType::Cancel);
                                },
                            )],
                        ),
                    )
                    .unwrap();

                *dialog = Some(components.root);
            }
            LoginType::Guest => {
                info!("guest");
                toaster.add_toast(
                    "login profile",
                    "Warning: Guest profile will not persist beyond the current session",
                );
                commands.send_event(SystemAudio(
                    "embedded://sounds/ui/toggle_enable.wav".to_owned(),
                ));
                bridge.write(SystemApi::LoginGuest);
            }
            LoginType::Cancel => {
                *req_code = None;
                *req_done = None;
                *dialog = None;
                commands.send_event(SystemAudio(
                    "embedded://sounds/ui/toggle_disable.wav".to_owned(),
                ));
                bridge.write(SystemApi::LoginCancel);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_profile_for_realm(
    realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut task: Local<Option<Task<Result<Option<UserProfile>, anyhow::Error>>>>,
    ipfas: IpfsAssetServer,
) {
    if realm.is_changed() && !wallet.is_guest() {
        if let Some(address) = wallet.address() {
            *task = Some(IoTaskPool::get().spawn_compat(get_remote_profile(
                address,
                ipfas.ipfs().clone(),
                None,
            )));
        }
    }

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(Some(profile))) => {
                current_profile.profile = Some(profile);
                current_profile.is_deployed = true;
                current_profile.deploy_held = false;
            }
            Some(Ok(None)) | Some(Err(_)) => {
                // keep existing profile
            }
            None => *task = Some(t),
        }
    }
}

fn get_previous_login(config: &AppConfig) -> Option<PreviousLogin> {
    let previous_login = config.previous_login.clone();

    let mut expired = false;
    if let Some(prev) = previous_login.as_ref() {
        for link in &prev.auth {
            if link.ty == "ECDSA_EPHEMERAL" {
                for line in link.payload.lines() {
                    if line.starts_with("Expiration:") {
                        let exp = line.split_once(':').unwrap().1;
                        if let Ok(exp) = chrono::DateTime::<chrono::Utc>::from_str(exp.trim()) {
                            let now: chrono::DateTime<chrono::Utc> =
                                chrono::DateTime::from_timestamp_millis(
                                    web_time::SystemTime::now()
                                        .duration_since(web_time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis() as i64,
                                )
                                .unwrap();
                            if now > exp {
                                warn!("previous login expired, removing");
                                expired = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if expired {
        None
    } else {
        previous_login
    }
}

/// Decode a base64(JSON) AuthIdentity into the pieces the wallet needs: the root (signer)
/// address, the ephemeral key (just its address when the page holds the key), and the delegate
/// auth chain (the ECDSA_EPHEMERAL link(s), i.e. the chain WITHOUT the SIGNER entry — matching
/// what `finish_remote_ephemeral_request` / `get_previous_login` use).
///
/// This is the standard Decentraland AuthIdentity, so it is identical regardless of how the
/// user signed in (wallet/MetaMask, social, OTP, magic). The web page just reads it from
/// localStorage and forwards it — there is nothing login-method-specific here.
fn parse_auth_identity(payload: &str) -> Result<(Address, EphemeralKey, Vec<ChainLink>), String> {
    let parts = wallet::browser_auth::parse_auth_identity(payload)
        .and_then(wallet::browser_auth::auth_identity_key_parts)
        .map_err(|e| e.to_string())?;
    if matches!(parts.1, EphemeralKey::Remote(_)) && !wallet::remote_signer::remote_sign_available()
    {
        return Err("identity has no ephemeral private key".to_owned());
    }
    Ok(parts)
}

/// Fetch the profile, retrying on failure. Ok(None) means the user has no profile (or
/// `default_on_error` was set); a persistent failure must otherwise fail the login rather
/// than fall back to a default profile, or we would deploy the default over the user's
/// existing server-side profile. Retries are patient because the realm may still be
/// resolving when login runs (on web the page can fire `/login_identity` at boot), which
/// reads as a fetch failure. Errors carry the PROFILE_FETCH_FAILED prefix so UIs can
/// offer retrying with `default_on_error`.
async fn get_profile_with_retry(
    address: Address,
    ipfs: std::sync::Arc<IpfsIo>,
    default_on_error: bool,
) -> Result<Option<UserProfile>, String> {
    const ATTEMPTS: u32 = 5;
    let mut last_error = String::default();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            async_std::task::sleep(std::time::Duration::from_secs(2)).await;
        }
        match get_remote_profile(address, ipfs.clone(), None).await {
            Ok(maybe_profile) => return Ok(maybe_profile),
            Err(e) => last_error = e.to_string(),
        }
    }
    if default_on_error {
        warn!("continuing with default profile after fetch failure: {last_error}");
        return Ok(None);
    }
    Err(format!("{PROFILE_FETCH_FAILED}: {last_error}"))
}

/// The profile a login lands with.
enum LoginProfile {
    /// On the server.
    Deployed(UserProfile),
    /// Built here for an account with none; deployed on entry, or by the welcome page.
    New(UserProfile),
}

async fn login_profile(
    root_address: Address,
    ipfs: std::sync::Arc<IpfsIo>,
    default_on_error: bool,
) -> Result<LoginProfile, String> {
    match get_profile_with_retry(root_address, ipfs.clone(), default_on_error).await? {
        Some(profile) => Ok(LoginProfile::Deployed(profile)),
        None => Ok(LoginProfile::New(
            new_account_profile(root_address, ipfs).await,
        )),
    }
}

fn new_profile(address: Address, ipfs: &IpfsIo) -> UserProfile {
    UserProfile {
        version: 0,
        content: SerializedProfile {
            has_connected_web3: Some(true),
            eth_address: format!("{address:#x}"),
            user_id: Some(format!("{address:#x}")),
            ..Default::default()
        },
        base_url: ipfs.contents_endpoint().unwrap_or_default(),
    }
}

/// Name parts for a new account. Every first + last pair must be alphanumeric and at most 15
/// characters, the unclaimed-name rule shared by the HUD, the account site and the other
/// clients (and their `@mention` patterns); see the test below.
const GUEST_FIRST_NAMES: &[&str] = &[
    "Astra", "Bolt", "Cinder", "Dusk", "Ember", "Flint", "Gale", "Haze", "Indigo", "Jade", "Kite",
    "Lumen", "Mica", "Nimbus", "Onyx", "Pixel", "Quill", "Rune", "Sable", "Talon", "Umber",
    "Vesper", "Wren", "Xenon", "Yarrow", "Zephyr", "Comet", "Delta", "Echo", "Nova",
];
const GUEST_LAST_NAMES: &[&str] = &[
    "Ashgrove",
    "Deepwater",
    "Fernbrook",
    "Frostvale",
    "Glassmoor",
    "Greywood",
    "Highmoor",
    "Ironwood",
    "Mistgrove",
    "Moonvale",
    "Northwind",
    "Oakhollow",
    "Rainfield",
    "Riverbend",
    "Starfall",
    "Stonehill",
    "Sunridge",
    "Thornwood",
    "Wildmarsh",
    "Windward",
];

/// The name and look are derived from the address so a new account comes back the same if the
/// first deploy didn't land; an EOA address is a keccak digest, so the bytes are uniform.
fn guest_name(address: &Address) -> String {
    let bytes = address.as_slice();
    format!(
        "{}{}",
        GUEST_FIRST_NAMES[bytes[4] as usize % GUEST_FIRST_NAMES.len()],
        GUEST_LAST_NAMES[bytes[5] as usize % GUEST_LAST_NAMES.len()]
    )
}

fn guest_look_index(address: &Address) -> u32 {
    let bytes = address.as_slice();
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % DEFAULT_LOOKS + 1
}

/// An account without a profile gets a name and one of the curated default looks, so it
/// doesn't start as every other new account. Falls back to the engine's default look if the
/// catalyst can't provide one.
async fn new_account_profile(address: Address, ipfs: std::sync::Arc<IpfsIo>) -> UserProfile {
    let index = guest_look_index(&address);
    let look = match get_default_look(ipfs.clone(), index).await {
        Ok(Some(look)) => Some(look.content.avatar),
        Ok(None) => {
            warn!("default look {index} not found");
            None
        }
        Err(e) => {
            warn!("default look {index}: {e}");
            None
        }
    };
    let mut profile = new_profile(address, &ipfs);
    profile.content.name = guest_name(&address);
    if let Some(look) = look {
        let own = &mut profile.content.avatar;
        own.body_shape = look.body_shape;
        own.wearables = look.wearables;
        own.force_render = look.force_render;
        own.eyes = look.eyes;
        own.hair = look.hair;
        own.skin = look.skin;
        // snapshots are the look's own; ours are empty until something renders them
    }
    profile
}

const ACCOUNT_LOCKED: &str = "the account cannot change after entering a realm";

fn account_locked(wallet: &Wallet, world_hold: &Option<Res<WorldHold>>) -> bool {
    world_hold.is_none() && wallet.address().is_some()
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn process_login_bridge(
    mut e: EventReader<SystemApi>,
    ipfas: IpfsAssetServer,
    mut login_task: Local<
        Option<
            Task<
                Result<
                    (
                        Address,
                        EphemeralKey,
                        Vec<ChainLink>,
                        bool,
                        LoginProfile,
                        RpcResultSender<Result<(), String>>,
                    ),
                    (),
                >,
            >,
        >,
    >,
    mut wallet: ResMut<Wallet>,
    mut segment_config: ResMut<SegmentConfig>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut config: ResMut<AppConfig>,
    world_hold: Option<Res<WorldHold>>,
) {
    for ev in e.read().cloned() {
        match ev {
            SystemApi::CheckForUpdate(rpc_result_sender) => IoTaskPool::get()
                .spawn(async move {
                    rpc_result_sender.send(check_update().await);
                })
                .detach(),
            SystemApi::MOTD(rpc_result_sender) => {
                rpc_result_sender.send("".to_owned());
            }
            SystemApi::GetPreviousLogin(rpc_result_sender) => {
                rpc_result_sender
                    .send(get_previous_login(&config).map(|pl| format!("{:#x}", pl.root_address)));
            }
            SystemApi::GetWelcome(sender) => sender.send(WelcomeState {
                terms: !config.terms_accepted(),
                new_profile: current_profile.deploy_held,
            }),
            // the welcome page awaits this before its save, which releases a held new profile
            SystemApi::AcceptTerms(sender) => {
                if !config.terms_accepted() {
                    config.accepted_terms = Some(TERMS_VERSION);
                    platform::write_config_file(&*config);
                }
                sender.send(());
            }
            // the account can only change while the world is held (the lobby): once in
            // world, comms and scenes are bound to the wallet that entered
            SystemApi::LoginPrevious(_, sender) | SystemApi::LoginWithIdentity(_, _, _, sender)
                if account_locked(&wallet, &world_hold) =>
            {
                sender.send(Err(ACCOUNT_LOCKED.to_owned()));
            }
            SystemApi::LoginNew(_, code_sender, result_sender)
                if account_locked(&wallet, &world_hold) =>
            {
                code_sender.send(Err(ACCOUNT_LOCKED.to_owned()));
                result_sender.send(Err(ACCOUNT_LOCKED.to_owned()));
            }
            SystemApi::LoginGuest | SystemApi::Logout if account_locked(&wallet, &world_hold) => {
                warn!("{ACCOUNT_LOCKED}");
            }
            SystemApi::LoginPrevious(default_on_error, rpc_result_sender) => {
                let ipfs = ipfas.ipfs().clone();
                let maybe_previous_login = get_previous_login(&config);
                *login_task = Some(IoTaskPool::get().spawn_compat(async move {
                    let Some(previous_login) = maybe_previous_login else {
                        rpc_result_sender.send(Err("No Previous Login Available".to_string()));
                        return Err(());
                    };

                    let PreviousLogin {
                        root_address,
                        ephemeral_key,
                        auth,
                        guest_account,
                    } = previous_login;

                    let profile = match login_profile(root_address, ipfs, default_on_error).await {
                        Ok(profile) => profile,
                        Err(e) => {
                            rpc_result_sender.send(Err(e));
                            return Err(());
                        }
                    };

                    let local_wallet =
                        PrivateKeySigner::from_bytes(&B256::from_slice(&ephemeral_key)).unwrap();

                    Ok((
                        previous_login.root_address,
                        EphemeralKey::Local(local_wallet),
                        auth,
                        guest_account,
                        profile,
                        rpc_result_sender,
                    ))
                }));
            }
            SystemApi::LoginNew(default_on_error, code_sender, result_sender) => {
                let ipfs = ipfas.ipfs().clone();
                *login_task = Some(IoTaskPool::get().spawn_compat(async move {
                    let req = init_remote_ephemeral_request().await;
                    let req = match req {
                        Err(e) => {
                            code_sender.send(Err(e.to_string()));
                            result_sender.send(Err(e.to_string()));
                            return Err(());
                        }
                        Ok(res) => res,
                    };

                    // the deep-link flow has no verification code
                    code_sender.send(Ok(None));

                    let (root_address, local_wallet, auth, _) =
                        match finish_remote_ephemeral_request(req).await {
                            Ok(res) => res,
                            Err(e) => {
                                code_sender.send(Err(e.to_string()));
                                result_sender.send(Err(e.to_string()));
                                return Err(());
                            }
                        };

                    let profile = match login_profile(root_address, ipfs, default_on_error).await {
                        Ok(profile) => profile,
                        Err(e) => {
                            result_sender.send(Err(e));
                            return Err(());
                        }
                    };

                    Ok((
                        root_address,
                        EphemeralKey::Local(local_wallet),
                        auth,
                        false,
                        profile,
                        result_sender,
                    ))
                }));
            }
            SystemApi::LoginWithIdentity(
                payload,
                default_on_error,
                guest_account,
                rpc_result_sender,
            ) => {
                // The web page already holds a signed AuthIdentity (read from localStorage,
                // produced by whatever sign-in method the user used) — finalize the wallet
                // from it directly, no auth-server request/poll. Mirrors LoginPrevious.
                let ipfs = ipfas.ipfs().clone();
                *login_task = Some(IoTaskPool::get().spawn_compat(async move {
                    let (root_address, ephemeral_key, auth) = match parse_auth_identity(&payload) {
                        Ok(parts) => parts,
                        Err(e) => {
                            rpc_result_sender.send(Err(e));
                            return Err(());
                        }
                    };

                    let profile = match login_profile(root_address, ipfs, default_on_error).await {
                        Ok(profile) => profile,
                        Err(e) => {
                            rpc_result_sender.send(Err(e));
                            return Err(());
                        }
                    };

                    Ok((
                        root_address,
                        ephemeral_key,
                        auth,
                        guest_account,
                        profile,
                        rpc_result_sender,
                    ))
                }));
            }
            SystemApi::LoginGuest => {
                *login_task = None;
                wallet.finalize_as_guest();
                segment_config.update_identity(format!("{:#x}", wallet.address().unwrap()), true);
                current_profile.profile = Some(UserProfile {
                    version: 0,
                    content: SerializedProfile {
                        eth_address: format!("{:#x}", wallet.address().unwrap()),
                        user_id: Some(format!("{:#x}", wallet.address().unwrap())),
                        ..Default::default()
                    },
                    base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                });
                current_profile.is_deployed = true;
                current_profile.deploy_held = false;
            }
            SystemApi::LoginCancel => {
                *login_task = None;
            }
            SystemApi::Logout => {
                *login_task = None;
                wallet.disconnect();
                current_profile.profile = None;
                current_profile.deploy_held = false;
            }
            _ => (),
        }
    }

    if let Some(mut task) = login_task.take() {
        match task.complete() {
            // started in the lobby over another account, finished after the world was entered
            Some(Ok((.., sender))) if account_locked(&wallet, &world_hold) => {
                sender.send(Err(ACCOUNT_LOCKED.to_owned()));
            }
            Some(Ok((root_address, ephemeral_key, auth, guest_account, profile, sender))) => {
                if let Ok(mut window) = window.single_mut() {
                    window.focused = true;
                }

                match ephemeral_key {
                    EphemeralKey::Local(local_wallet) => {
                        let ephemeral_key = local_wallet.to_bytes().to_vec();

                        // store to app config
                        config.previous_login = Some(PreviousLogin {
                            root_address,
                            ephemeral_key,
                            auth: auth.clone(),
                            guest_account,
                        });
                        platform::write_config_file(&*config);

                        wallet.finalize(root_address, local_wallet, auth, guest_account);
                    }
                    // the page holds the key, so there is none to store, and none from an
                    // earlier login should stay behind
                    EphemeralKey::Remote(ephemeral_address) => {
                        if config.previous_login.take().is_some() {
                            platform::write_config_file(&*config);
                        }
                        wallet.finalize_remote(
                            root_address,
                            ephemeral_address,
                            auth,
                            guest_account,
                        );
                    }
                }
                segment_config
                    .update_identity(format!("{:#x}", wallet.address().unwrap()), guest_account);
                let (profile, deployed) = match profile {
                    LoginProfile::Deployed(profile) => (profile, true),
                    LoginProfile::New(profile) => (profile, false),
                };
                current_profile.profile = Some(profile);
                current_profile.is_deployed = deployed;
                // held, the host shows its welcome page, which deploys the new profile
                current_profile.deploy_held = !deployed && world_hold.is_some();

                sender.send(Ok(()));
            }
            Some(Err(())) => (),
            None => *login_task = Some(task),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_names_are_valid_unclaimed_names() {
        for first in GUEST_FIRST_NAMES {
            for last in GUEST_LAST_NAMES {
                let name = format!("{first}{last}");
                assert!(
                    name.len() <= 15 && name.chars().all(|c| c.is_ascii_alphanumeric()),
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn guest_look_is_a_default_profile_pointer() {
        for address in [
            Address::ZERO,
            Address::repeat_byte(0xff),
            Address::repeat_byte(0x7a),
        ] {
            let index = guest_look_index(&address);
            assert!((1..=DEFAULT_LOOKS).contains(&index), "{index}");
        }
    }
}
