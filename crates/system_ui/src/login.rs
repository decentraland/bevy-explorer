use std::str::FromStr;

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
        ZOrder,
    },
    util::{TaskCompat, TaskExt},
};
use comms::profile::{get_remote_profile, CurrentUserProfile, UserProfile};
use ethers_core::types::Address;
use ethers_signers::LocalWallet;
use ipfs::{IpfsAssetServer, IpfsIo};
use scene_runner::Toaster;
use system_bridge::{NativeUi, SystemApi, PROFILE_FETCH_FAILED};
use tokio::sync::oneshot::error::TryRecvError;
use ui_core::{
    button::DuiButton,
    ui_actions::{close_ui_happy, Click, EventCloneExt, On},
};
use wallet::{
    browser_auth::{finish_remote_ephemeral_request, init_remote_ephemeral_request},
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
            Ok(Ok(code)) => {
                if let Some(mut commands) = dialog.and_then(|d| commands.get_entity(d).ok()) {
                    commands.despawn();
                    *dialog = None;
                }

                let components = commands
                    .spawn(ZOrder::Login.default())
                    .apply_template(
                        &dui,
                        "cancel-login",
                        DuiProps::new()
                            .with_prop(
                                "buttons",
                                vec![DuiButton::new_enabled(
                                    "Cancel",
                                    |mut e: EventWriter<LoginType>| {
                                        e.write(LoginType::Cancel);
                                    },
                                )],
                            )
                            .with_prop("code", format!("{}", code.unwrap_or(-1))),
                    )
                    .unwrap();

                *dialog = Some(components.root);
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
                        DuiProps::new()
                            .with_prop(
                                "buttons",
                                vec![DuiButton::new_enabled(
                                    "Cancel",
                                    |mut e: EventWriter<LoginType>| {
                                        e.write(LoginType::Cancel);
                                    },
                                )],
                            )
                            .with_prop("code", "...".to_string()),
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
/// address, the ephemeral LocalWallet, and the delegate auth chain (the ECDSA_EPHEMERAL
/// link(s), i.e. the chain WITHOUT the SIGNER entry — matching what
/// `finish_remote_ephemeral_request` / `get_previous_login` use).
///
/// This is the standard Decentraland AuthIdentity, so it is identical regardless of how the
/// user signed in (wallet/MetaMask, social, OTP, magic). The web page just reads it from
/// localStorage and forwards it — there is nothing login-method-specific here.
fn parse_auth_identity(payload: &str) -> Result<(Address, LocalWallet, Vec<ChainLink>), String> {
    use base64::Engine as _;

    #[derive(serde::Deserialize)]
    struct Ephemeral {
        #[serde(rename = "privateKey")]
        private_key: String,
    }
    #[derive(serde::Deserialize)]
    struct AuthIdentity {
        #[serde(rename = "ephemeralIdentity")]
        ephemeral_identity: Ephemeral,
        #[serde(rename = "authChain")]
        auth_chain: Vec<ChainLink>,
    }

    let json = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| format!("bad identity base64: {e}"))?;
    let identity: AuthIdentity =
        serde_json::from_slice(&json).map_err(|e| format!("bad identity json: {e}"))?;

    // Root wallet address = the SIGNER link's payload.
    let signer = identity
        .auth_chain
        .iter()
        .find(|l| l.ty == "SIGNER")
        .ok_or_else(|| "identity missing SIGNER link".to_string())?;
    let root_address =
        Address::from_str(signer.payload.trim()).map_err(|e| format!("bad root address: {e}"))?;

    // Ephemeral signer from the 0x-prefixed private key.
    let key_hex = identity
        .ephemeral_identity
        .private_key
        .trim()
        .trim_start_matches("0x");
    let local_wallet =
        LocalWallet::from_str(key_hex).map_err(|e| format!("bad ephemeral key: {e}"))?;

    // Delegate chain = everything except the SIGNER (the ECDSA_EPHEMERAL link the engine stores).
    let auth: Vec<ChainLink> = identity
        .auth_chain
        .into_iter()
        .filter(|l| l.ty != "SIGNER")
        .collect();
    if auth.is_empty() {
        return Err("identity missing ephemeral delegate link".to_string());
    }

    Ok((root_address, local_wallet, auth))
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
                        LocalWallet,
                        Vec<ChainLink>,
                        Option<UserProfile>,
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
                    } = previous_login;

                    let profile =
                        match get_profile_with_retry(root_address, ipfs, default_on_error).await {
                            Ok(maybe_profile) => maybe_profile,
                            Err(e) => {
                                rpc_result_sender.send(Err(e));
                                return Err(());
                            }
                        };

                    let local_wallet = LocalWallet::from_bytes(&ephemeral_key).unwrap();

                    Ok((
                        previous_login.root_address,
                        local_wallet,
                        auth,
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

                    code_sender.send(Ok(req.code));

                    let (root_address, local_wallet, auth, _) =
                        match finish_remote_ephemeral_request(req).await {
                            Ok(res) => res,
                            Err(e) => {
                                code_sender.send(Err(e.to_string()));
                                result_sender.send(Err(e.to_string()));
                                return Err(());
                            }
                        };

                    let profile =
                        match get_profile_with_retry(root_address, ipfs, default_on_error).await {
                            Ok(maybe_profile) => maybe_profile,
                            Err(e) => {
                                result_sender.send(Err(e));
                                return Err(());
                            }
                        };

                    Ok((root_address, local_wallet, auth, profile, result_sender))
                }));
            }
            SystemApi::LoginWithIdentity(payload, default_on_error, rpc_result_sender) => {
                // The web page already holds a signed AuthIdentity (read from localStorage,
                // produced by whatever sign-in method the user used) — finalize the wallet
                // from it directly, no auth-server request/poll. Mirrors LoginPrevious.
                let ipfs = ipfas.ipfs().clone();
                *login_task = Some(IoTaskPool::get().spawn_compat(async move {
                    let (root_address, local_wallet, auth) = match parse_auth_identity(&payload) {
                        Ok(parts) => parts,
                        Err(e) => {
                            rpc_result_sender.send(Err(e));
                            return Err(());
                        }
                    };

                    let profile =
                        match get_profile_with_retry(root_address, ipfs, default_on_error).await {
                            Ok(maybe_profile) => maybe_profile,
                            Err(e) => {
                                rpc_result_sender.send(Err(e));
                                return Err(());
                            }
                        };

                    Ok((root_address, local_wallet, auth, profile, rpc_result_sender))
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
            }
            SystemApi::LoginCancel => {
                *login_task = None;
            }
            SystemApi::Logout => {
                *login_task = None;
                wallet.disconnect();
                current_profile.profile = None;
            }
            _ => (),
        }
    }

    if let Some(mut task) = login_task.take() {
        match task.complete() {
            Some(Ok((root_address, local_wallet, auth, profile, sender))) => {
                if let Ok(mut window) = window.single_mut() {
                    window.focused = true;
                }

                let ephemeral_key = local_wallet.signer().to_bytes().to_vec();

                // store to app config
                config.previous_login = Some(PreviousLogin {
                    root_address,
                    ephemeral_key,
                    auth: auth.clone(),
                });
                platform::write_config_file(&*config);

                wallet.finalize(root_address, local_wallet, auth);
                segment_config.update_identity(format!("{:#x}", wallet.address().unwrap()), false);
                if let Some(profile) = profile {
                    current_profile.profile = Some(profile);
                    current_profile.is_deployed = true;
                } else {
                    current_profile.profile = Some(UserProfile {
                        version: 0,
                        content: SerializedProfile {
                            has_connected_web3: Some(true),
                            eth_address: format!("{:#x}", wallet.address().unwrap()),
                            user_id: Some(format!("{:#x}", wallet.address().unwrap())),
                            ..Default::default()
                        },
                        base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                    });
                    current_profile.is_deployed = false;
                }

                sender.send(Ok(()));
            }
            Some(Err(())) => (),
            None => *login_task = Some(task),
        }
    }
}
