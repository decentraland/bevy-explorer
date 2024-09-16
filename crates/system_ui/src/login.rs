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
    structs::{ActiveDialog, AppConfig, ChainLink, DialogPermit, PreviousLogin, SystemAudio},
    util::{config_file, FireEventEx, TaskExt},
};
use comms::{
    preview::PreviewMode,
    profile::{get_remote_profile, CurrentUserProfile, UserProfile},
};
use ethers_core::types::Address;
use ethers_signers::LocalWallet;
use ipfs::{CurrentRealm, IpfsAssetServer};
use scene_runner::Toaster;
use ui_core::{
    button::DuiButton,
    ui_actions::{close_ui_happy, Click, EventCloneExt, On},
};
use wallet::{
    browser_auth::{
        finish_remote_ephemeral_request, init_remote_ephemeral_request, RemoteEphemeralRequest,
    },
    Wallet,
};

use crate::version_check::check_update;

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoginType>().add_systems(
            Update,
            (login, update_profile_for_realm).run_if(in_state(ui_core::State::Ready)),
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
    ipfas: IpfsAssetServer,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut init_task: Local<Option<Task<Result<RemoteEphemeralRequest, anyhow::Error>>>>,
    mut segment_config: ResMut<SegmentConfig>,
    mut final_task: Local<
        Option<
            Task<
                Result<(Address, LocalWallet, Vec<ChainLink>, Option<UserProfile>), anyhow::Error>,
            >,
        >,
    >,
    mut logins: EventReader<LoginType>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
    dui: Res<DuiRegistry>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    _preview: Res<PreviewMode>,
    _config: Res<AppConfig>,
    active_dialog: Res<ActiveDialog>,
    mut motd_shown: Local<bool>,
) {
    if !*motd_shown {
        let update = check_update();
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
        *final_task = None;
        return;
    }

    // auto-login in preview mode disabled for now
    /*
    if preview.server.is_some() && final_task.is_none() {
        if let Some(previous_login) = config.previous_login.clone() {
            let ipfs = ipfas.ipfs().clone();
            *final_task = Some(IoTaskPool::get().spawn(async move {
                let PreviousLogin {
                    root_address,
                    ephemeral_key,
                    auth,
                } = previous_login;

                let profile = get_remote_profile(root_address, ipfs).await.ok();
                let local_wallet = LocalWallet::from_bytes(&ephemeral_key).unwrap();

                Ok((previous_login.root_address, local_wallet, auth, profile))
            }));
        } else {
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
            return;
        }
    }
    */

    // create dialog
    if dialog.is_none() && final_task.is_none() {
        let Some(permit) = active_dialog.try_acquire() else {
            return;
        };

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

        let previous_login = if expired { None } else { previous_login };

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
    if let Some(mut t) = init_task.take() {
        match t.complete() {
            Some(Ok(request)) => {
                let code = request.code;
                let ipfs = ipfas.ipfs().clone();

                *final_task = Some(IoTaskPool::get().spawn(async move {
                    let (root_address, local_wallet, auth, _) =
                        finish_remote_ephemeral_request(request).await?;

                    let profile = get_remote_profile(root_address, ipfs).await.ok();

                    Ok((root_address, local_wallet, auth, profile))
                }));

                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
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
            Some(Err(e)) => {
                error!("{e}");
                toaster.add_toast("login profile", format!("Login failed: {}", e));
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
                }
                *dialog = None;
            }
            None => {
                *init_task = Some(t);
            }
        }
    }
    if let Some(mut t) = final_task.take() {
        match t.complete() {
            Some(Ok((root_address, local_wallet, auth, profile))) => {
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
                    toaster.add_toast("login profile", "Profile loaded");
                    current_profile.profile = Some(profile);
                    current_profile.is_deployed = true;
                } else {
                    toaster.add_toast("login profile", "Failed to load profile, using default");
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
            }
            Some(Err(e)) => {
                error!("{e}");
                toaster.add_toast("login profile", format!("Login failed: {}", e));
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
                }
                *dialog = None;
            }
            None => {
                *final_task = Some(t);
            }
        }
    }

    // handle click
    if let Some(login) = logins.read().last() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }

        match login {
            LoginType::ExistingRemote => {
                info!("existing remote");
                let ipfs = ipfas.ipfs().clone();
                let previous_login = std::fs::read(config_file())
                    .ok()
                    .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
                    .unwrap()
                    .previous_login
                    .unwrap();

                *final_task = Some(IoTaskPool::get().spawn(async move {
                    let PreviousLogin {
                        root_address,
                        ephemeral_key,
                        auth,
                    } = previous_login;

                    let profile = get_remote_profile(root_address, ipfs).await.ok();

                    let local_wallet = LocalWallet::from_bytes(&ephemeral_key).unwrap();

                    Ok((previous_login.root_address, local_wallet, auth, profile))
                }));
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
            }
            LoginType::NewRemote => {
                info!("new remote");

                *init_task = Some(IoTaskPool::get().spawn(init_remote_ephemeral_request()));

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
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
            }
            LoginType::Guest => {
                info!("guest");
                toaster.add_toast(
                    "login profile",
                    "Warning: Guest profile will not persist beyond the current session",
                );
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
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
            }
            LoginType::Cancel => {
                *final_task = None;
                *dialog = None;
                commands.fire_event(SystemAudio("sounds/ui/toggle_disable.wav".to_owned()));
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
            *task =
                Some(IoTaskPool::get().spawn(get_remote_profile(address, ipfas.ipfs().clone())));
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
