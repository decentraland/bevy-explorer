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
    rpc::RpcResultSender,
    structs::{ActiveDialog, AppConfig, ChainLink, DialogPermit, PreviousLogin, SystemAudio},
    util::{config_file, FireEventEx, TaskCompat, TaskExt},
};
use comms::profile::{get_remote_profile, CurrentUserProfile, UserProfile};
use ethers_core::types::Address;
use ethers_signers::LocalWallet;
use ipfs::{CurrentRealm, IpfsAssetServer};
use scene_runner::Toaster;
use system_bridge::{NativeUi, SystemApi};
use tokio::sync::oneshot::error::TryRecvError;
use ui_core::{
    button::DuiButton,
    ui_actions::{close_ui_happy, Click, EventCloneExt, On},
};
use wallet::{
    browser_auth::{finish_remote_ephemeral_request, init_remote_ephemeral_request},
    Wallet,
};

use crate::version_check::{check_update, check_update_sync};

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoginType>().add_systems(
            Update,
            (
                (login, update_profile_for_realm).run_if(in_state(ui_core::State::Ready)),
                process_system_bridge,
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

type RpcReceiver<T> = tokio::sync::oneshot::Receiver<T>;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn login(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut req_code: Local<Option<RpcReceiver<Result<Option<i32>, String>>>>,
    mut req_done: Local<Option<RpcReceiver<Result<(), String>>>>,
    mut logins: EventReader<LoginType>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
    dui: Res<DuiRegistry>,
    active_dialog: Res<ActiveDialog>,
    mut motd_shown: Local<bool>,
    mut bridge: EventWriter<SystemApi>,
    native_active: Res<NativeUi>,
) {
    if !native_active.login {
        return;
    }

    if !*motd_shown {
        let update = check_update_sync();
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
                            let mut permit = permit.single_mut();
                            let permit = permit.take();
                            let components = commands
                                .spawn_template(
                                    &dui,
                                    "motd",
                                    DuiProps::default()
                                        .with_prop("buttons", vec![DuiButton::new_enabled("Ok", close_ui_happy)]),
                                )
                                .unwrap();
                            commands.entity(components.root).insert(permit);
                        }).pipe(close_ui_happy))]),
                )
                .unwrap();
            commands.entity(components.root).insert(permit);
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
            commands.entity(components.root).insert(permit);
        }
        *motd_shown = true;
        return;
    }

    // cleanup if we're done
    if wallet.address().is_some() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
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

        let previous_login = get_previous_login();

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
                        e.send_default();
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
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
                    *dialog = None;
                }

                let components = commands
                    .spawn_template(
                        &dui,
                        "cancel-login",
                        DuiProps::new()
                            .with_prop(
                                "buttons",
                                vec![DuiButton::new_enabled(
                                    "Cancel",
                                    |mut e: EventWriter<LoginType>| {
                                        e.send(LoginType::Cancel);
                                    },
                                )],
                            )
                            .with_prop("code", format!("{}", code.unwrap_or(-1))),
                    )
                    .unwrap();

                *dialog = Some(components.root);
            }
            Ok(Err(e)) => {
                toaster.add_toast("login profile", format!("Login failed: {}", e));
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
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
                toaster.add_toast("login profile", format!("Login failed: {}", e));
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
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
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
            *dialog = None;
        }

        match login {
            LoginType::ExistingRemote => {
                info!("existing remote");
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                bridge.send(SystemApi::LoginPrevious(sx.into()));
                *req_done = Some(rx);
            }
            LoginType::NewRemote => {
                info!("new remote");

                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                let (scode, rcode) = tokio::sync::oneshot::channel::<Result<Option<i32>, String>>();
                let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                bridge.send(SystemApi::LoginNew(scode.into(), sx.into()));
                *req_code = Some(rcode);
                *req_done = Some(rx);

                let components = commands
                    .spawn_template(
                        &dui,
                        "cancel-login",
                        DuiProps::new()
                            .with_prop(
                                "buttons",
                                vec![DuiButton::new_enabled(
                                    "Cancel",
                                    |mut e: EventWriter<LoginType>| {
                                        e.send(LoginType::Cancel);
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
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                bridge.send(SystemApi::LoginGuest);
            }
            LoginType::Cancel => {
                *req_code = None;
                *req_done = None;
                *dialog = None;
                commands.fire_event(SystemAudio("sounds/ui/toggle_disable.wav".to_owned()));
                bridge.send(SystemApi::LoginCancel);
            }
        }
    }
}

fn update_profile_for_realm(
    realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut task: Local<Option<Task<Result<UserProfile, anyhow::Error>>>>,
    ipfas: IpfsAssetServer,
) {
    if realm.is_changed() && !wallet.is_guest() {
        if let Some(address) = wallet.address() {
            *task = Some(IoTaskPool::get().spawn(get_remote_profile(
                address,
                ipfas.ipfs().clone(),
                None,
            )));
        }
    }

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(profile)) => {
                current_profile.profile = Some(profile);
                current_profile.is_deployed = true;
            }
            Some(Err(_)) => {
                current_profile.profile = Some(UserProfile {
                    version: 0,
                    content: SerializedProfile {
                        has_connected_web3: Some(true),
                        ..Default::default()
                    },
                    base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                });
            }
            None => *task = Some(t),
        }
    }
}

fn get_previous_login() -> Option<PreviousLogin> {
    let previous_login = std::fs::read(config_file())
        .ok()
        .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
        .unwrap_or_default()
        .previous_login;

    let mut expired = false;
    if let Some(prev) = previous_login.as_ref() {
        for link in &prev.auth {
            if link.ty == "ECDSA_EPHEMERAL" {
                for line in link.payload.lines() {
                    if line.starts_with("Expiration:") {
                        let exp = line.split_once(':').unwrap().1;
                        if let Ok(exp) = chrono::DateTime::<chrono::Utc>::from_str(exp.trim()) {
                            let now: chrono::DateTime<chrono::Utc> =
                                std::time::SystemTime::now().into();
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

#[allow(clippy::type_complexity)]
fn process_system_bridge(
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
                    .send(get_previous_login().map(|pl| format!("{:#x}", pl.root_address)));
            }
            SystemApi::LoginPrevious(rpc_result_sender) => {
                let ipfs = ipfas.ipfs().clone();
                *login_task = Some(IoTaskPool::get().spawn(async move {
                    let Some(previous_login) = get_previous_login() else {
                        rpc_result_sender.send(Err("No Previous Login Available".to_string()));
                        return Err(());
                    };

                    let PreviousLogin {
                        root_address,
                        ephemeral_key,
                        auth,
                    } = previous_login;

                    let profile = get_remote_profile(root_address, ipfs, None).await.ok();

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
            SystemApi::LoginNew(code_sender, result_sender) => {
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

                    let profile = get_remote_profile(root_address, ipfs, None).await.ok();

                    Ok((root_address, local_wallet, auth, profile, result_sender))
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
                if let Ok(mut window) = window.get_single_mut() {
                    window.focused = true;
                }

                let ephemeral_key = local_wallet.signer().to_bytes().to_vec();

                // store to app config
                let mut config: AppConfig = std::fs::read(config_file())
                    .ok()
                    .and_then(|f| serde_json::from_slice(&f).ok())
                    .unwrap_or_default();
                config.previous_login = Some(PreviousLogin {
                    root_address,
                    ephemeral_key,
                    auth: auth.clone(),
                });
                let config_file = config_file();
                if let Some(folder) = config_file.parent() {
                    std::fs::create_dir_all(folder).unwrap();
                }
                if let Err(e) = std::fs::write(config_file, serde_json::to_string(&config).unwrap())
                {
                    warn!("failed to write to config: {e}");
                }

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
